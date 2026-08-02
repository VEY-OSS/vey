/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, anyhow};
use hdrhistogram::Histogram;
use serde_json::Value;
use tokio::sync::{Barrier, Semaphore, mpsc};
use tokio::time::{Instant, MissedTickBehavior};

use vey_statsd_client::StatsdClient;
use vey_types::limit::RateLimiter;

use super::ProcArgs;
use crate::report::{self, JsonObject, insert, json_usize, keys};

mod stats;

pub mod dns;
pub mod h1;
pub mod h2;
pub mod keyless;
pub mod openssl;
pub mod rustls;
pub mod thrift;
pub mod websocket;

#[cfg_attr(feature = "quic", path = "h3/mod.rs")]
#[cfg_attr(not(feature = "quic"), path = "no_h3.rs")]
pub mod h3;

const QUANTILE: &str = "quantile";

pub(crate) trait BenchHistogram {
    fn refresh(&mut self);
    fn emit(&self, client: &mut StatsdClient);

    fn emit_histogram(&self, client: &mut StatsdClient, histogram: &Histogram<u64>, key: &str) {
        let min = histogram.min();
        client.gauge(key, min).with_tag(QUANTILE, "min").send();
        let max = histogram.max();
        client.gauge(key, max).with_tag(QUANTILE, "max").send();
        let mean = histogram.mean();
        client
            .gauge_float(key, mean)
            .with_tag(QUANTILE, "mean")
            .send();
        let pct50 = histogram.value_at_quantile(0.50);
        client.gauge(key, pct50).with_tag(QUANTILE, "0.50").send();
        let pct80 = histogram.value_at_quantile(0.80);
        client.gauge(key, pct80).with_tag(QUANTILE, "0.80").send();
        let pct90 = histogram.value_at_quantile(0.90);
        client.gauge(key, pct90).with_tag(QUANTILE, "0.90").send();
        let pct95 = histogram.value_at_quantile(0.95);
        client.gauge(key, pct95).with_tag(QUANTILE, "0.95").send();
        let pct98 = histogram.value_at_quantile(0.98);
        client.gauge(key, pct98).with_tag(QUANTILE, "0.98").send();
        let pct99 = histogram.value_at_quantile(0.99);
        client.gauge(key, pct99).with_tag(QUANTILE, "0.99").send();
    }

    fn summary(&self);

    /// Target-specific histogram fields for `--json-file` output.
    fn json_report(&self) -> JsonObject;
}

pub(crate) trait BenchRuntimeStats {
    fn emit(&self, client: &mut StatsdClient);
    fn summary(&self, total_time: Duration);

    /// Target-specific runtime fields for `--json-file` output.
    fn json_report(&self, total_time: Duration) -> JsonObject;
}

enum BenchError {
    Fatal(anyhow::Error),
    Task(anyhow::Error),
}

trait BenchTaskContext {
    fn mark_task_start(&self);
    fn mark_task_passed(&self);
    fn mark_task_failed(&self);

    // TODO use native async fn declaration
    fn run(
        &mut self,
        task_id: usize,
        time_started: Instant,
    ) -> impl Future<Output = Result<(), BenchError>> + Send;
}

trait BenchTarget<RS, H, C>
where
    RS: BenchRuntimeStats,
    H: BenchHistogram,
    C: BenchTaskContext,
{
    fn new_context(&self) -> anyhow::Result<C>;
    fn fetch_runtime_stats(&self) -> Arc<RS>;
    fn take_histogram(&mut self) -> Option<H>;

    fn notify_finish(&mut self) {}
}

fn register_signal_handler() {
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            eprintln!("error when waiting Ctrl-C: {e}");
        }
        stats::mark_force_quit();
    });
}

async fn run<RS, H, C, T>(
    mut target: T,
    proc_args: &ProcArgs,
    target_name: &'static str,
) -> anyhow::Result<ExitCode>
where
    RS: BenchRuntimeStats + Send + Sync + 'static,
    H: BenchHistogram + Send + 'static,
    C: BenchTaskContext + Send + 'static,
    T: BenchTarget<RS, H, C> + Send + Sync + 'static,
{
    let sync_sem = Arc::new(Semaphore::new(0));
    let sync_barrier = Arc::new(Barrier::new(proc_args.concurrency.get() + 1));
    let (sender, mut receiver) = mpsc::channel::<usize>(proc_args.concurrency.get());
    let progress = proc_args.new_progress_bar();
    let progress_counter = progress.as_ref().map(|p| p.counter());

    stats::init_global_state(proc_args.requests, proc_args.log_error_count);
    register_signal_handler();

    let rate_limit = proc_args
        .rate_limit
        .map(|q| Arc::new(RateLimiter::new_global(q)));
    for i in 0..proc_args.concurrency.get() {
        let sem = Arc::clone(&sync_sem);
        let barrier = Arc::clone(&sync_barrier);
        let quit_sender = sender.clone();
        let progress_counter = progress_counter.clone();

        let mut context = target
            .new_context()
            .context(format!("failed to to create context #{i}"))?;

        let task_unconstrained = proc_args.task_unconstrained;
        let latency = proc_args.latency;
        let ignore_fatal_error = proc_args.ignore_fatal_error;
        let rate_limit = rate_limit.clone();
        let rt = super::worker::select_handle(i).unwrap_or_else(tokio::runtime::Handle::current);
        rt.spawn(async move {
            sem.add_permits(1);
            barrier.wait().await;

            let mut latency_interval = if let Some(latency) = latency {
                let mut interval = tokio::time::interval(latency);
                interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
                Some(interval)
            } else {
                None
            };

            let global_state = stats::global_state();
            let mut req_count = 0;
            while let Some(task_id) = global_state.fetch_request() {
                if let Some(latency) = &mut latency_interval {
                    latency.tick().await;
                }

                if let Some(r) = &rate_limit {
                    while let Err(t) = r.check() {
                        tokio::time::sleep(t).await;
                    }
                }

                let time_start = Instant::now();
                context.mark_task_start();
                let rt = if task_unconstrained {
                    tokio::task::unconstrained(context.run(task_id, time_start)).await
                } else {
                    context.run(task_id, time_start).await
                };
                match rt {
                    Ok(_) => {
                        context.mark_task_passed();
                        if let Some(c) = progress_counter.as_ref() {
                            c.inc();
                        }
                        global_state.add_passed();
                    }
                    Err(BenchError::Fatal(e)) => {
                        context.mark_task_failed();
                        global_state.add_failed();
                        if ignore_fatal_error {
                            if global_state.check_log_error() {
                                eprintln!("! request {task_id} failed: {e:?}\n");
                            }
                        } else {
                            eprintln!("!! Fatal error with task context {i}: {e:?}");
                            break;
                        }
                    }
                    Err(BenchError::Task(e)) => {
                        context.mark_task_failed();
                        global_state.add_failed();
                        if global_state.check_log_error() {
                            eprintln!("! request {task_id} failed: {e:?}\n");
                        }
                    }
                }
                req_count += 1;
            }

            drop(context);
            if let Err(e) = quit_sender.send(req_count).await {
                eprintln!("failed to send quit signal: {e}");
            }
        });
    }
    drop(sender);

    let _run_permit = sync_sem
        .acquire_many(proc_args.concurrency.get() as u32)
        .await
        .context("failed to start all task contexts")?;

    let quit_notifier = Arc::new(AtomicBool::new(false));
    // progress bar
    let progress_bar_handler = if let Some(progress) = progress {
        let handler = progress.spawn(quit_notifier.clone())?;
        Some(handler)
    } else {
        None
    };
    // simple runtime stats
    let runtime_stats_handler =
        if let Some((mut statsd_client, emit_interval)) = proc_args.new_statsd_client() {
            let runtime_stats = target.fetch_runtime_stats();
            let quit_notifier = quit_notifier.clone();
            let handler = std::thread::Builder::new()
                .name("runtime-stats".into())
                .spawn(move || {
                    loop {
                        runtime_stats.emit(&mut statsd_client);
                        statsd_client.flush_sink();

                        if quit_notifier.load(Ordering::Relaxed) {
                            break;
                        }

                        std::thread::sleep(emit_interval);
                    }
                })
                .map_err(|e| anyhow!("failed to create runtime stats thread: {e}"))?;
            Some(handler)
        } else {
            None
        };
    // histogram runtime stats
    let histogram_stats_handler = if let Some(mut histogram) = target.take_histogram() {
        let quit_notifier = quit_notifier.clone();
        let thread_builder = std::thread::Builder::new().name("histogram".into());
        if let Some((mut statsd_client, emit_interval)) = proc_args.new_statsd_client() {
            let handler = thread_builder
                .spawn(move || {
                    loop {
                        histogram.refresh();
                        histogram.emit(&mut statsd_client);

                        if quit_notifier.load(Ordering::Relaxed) {
                            break;
                        }

                        std::thread::sleep(emit_interval);
                    }
                    histogram
                })
                .map_err(|e| anyhow!("failed to create histogram metrics thread: {e}"))?;
            Some(handler)
        } else {
            let handler = thread_builder
                .spawn(move || {
                    loop {
                        histogram.refresh();

                        if quit_notifier.load(Ordering::Relaxed) {
                            break;
                        }

                        std::thread::sleep(Duration::from_millis(100));
                    }
                    histogram
                })
                .map_err(|e| anyhow!("failed to create histogram refresh thread: {e}"))?;
            Some(handler)
        }
    } else {
        None
    };

    let time_start = Instant::now();
    sync_barrier.wait().await;

    if let Some(time_limit) = proc_args.time_limit {
        std::thread::Builder::new()
            .name("quit-timer".into())
            .spawn(move || {
                std::thread::sleep(time_limit);
                stats::mark_force_quit();
            })
            .map_err(|e| anyhow!("failed to create quit timer thread: {e}"))?;
    }

    let mut distribute_histogram = Histogram::<u64>::new(3).unwrap();
    while let Some(req_count) = receiver.recv().await {
        distribute_histogram.record(req_count as u64).unwrap();
    }
    let total_time = time_start.elapsed();

    quit_notifier.store(true, Ordering::Relaxed);

    if let Some(handler) = progress_bar_handler {
        match handler.join() {
            Ok(bar) => bar.finish(),
            Err(e) => eprintln!("error to join progress bar thread: {e:?}"),
        }
    }

    if let Some(handler) = runtime_stats_handler {
        let _ = handler.join();
    }
    target.notify_finish();

    let distribution = if proc_args.concurrency.get() > 1 {
        Some(&distribute_histogram)
    } else {
        None
    };

    if !proc_args.no_summary {
        stats::global_state().summary(total_time, distribution);
        target.fetch_runtime_stats().summary(total_time);
    }

    let mut histogram_json = JsonObject::new();
    if let Some(handler) = histogram_stats_handler {
        match handler.join() {
            Ok(mut histogram) => {
                histogram.refresh();
                if !proc_args.no_summary {
                    histogram.summary();
                }
                if proc_args.json_file.is_some() {
                    histogram_json = histogram.json_report();
                }
            }
            Err(e) => eprintln!("error to join histogram stats thread: {e:?}"),
        }
    }

    if let Some(path) = &proc_args.json_file {
        let mut root = JsonObject::new();
        insert(&mut root, keys::VERSION, Value::Number(1.into()));
        insert(
            &mut root,
            keys::TARGET,
            Value::String(target_name.to_string()),
        );
        insert(
            &mut root,
            keys::CONCURRENCY,
            json_usize(proc_args.concurrency.get()),
        );
        insert(
            &mut root,
            keys::GLOBAL,
            stats::global_state().json_report(total_time, distribution),
        );

        for (key, value) in target.fetch_runtime_stats().json_report(total_time) {
            root.insert(key, value);
        }
        for (key, value) in histogram_json {
            root.insert(key, value);
        }

        report::write_json_file(path, &Value::Object(root))?;
    }

    let exit_code = if stats::global_state().all_succeeded() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    };
    Ok(exit_code)
}
