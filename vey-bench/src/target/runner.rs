/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::marker::PhantomData;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc as std_mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, anyhow};
use hdrhistogram::Histogram;
use serde_json::Value;
use tokio::sync::{Barrier, Semaphore, mpsc};
use tokio::time::{Instant, MissedTickBehavior};

use vey_types::limit::{GlobalRateLimitState, RateLimiter};

use super::{
    BenchError, BenchHistogram, BenchRuntimeStats, BenchTarget, BenchTaskContext, ProcArgs, stats,
};
use crate::progress::{BenchProgress, ProgressCounter};
use crate::report;

/// Runs a bench target, and can be run repeatedly with the same target.
pub(super) struct BenchRunner<RS, H, C, T> {
    target: T,
    proc_args: Arc<ProcArgs>,
    target_name: &'static str,
    histogram: Option<H>,
    finished: bool,
    _phantom: PhantomData<fn() -> (RS, C)>,
}

impl<RS, H, C, T> BenchRunner<RS, H, C, T>
where
    RS: BenchRuntimeStats + Send + Sync + 'static,
    H: BenchHistogram + Send + 'static,
    C: BenchTaskContext + Send + 'static,
    T: BenchTarget<RS, H, C> + Send + Sync + 'static,
{
    pub(super) fn new(mut target: T, proc_args: &Arc<ProcArgs>, target_name: &'static str) -> Self {
        register_signal_handler();
        let histogram = target.take_histogram();
        BenchRunner {
            target,
            proc_args: Arc::clone(proc_args),
            target_name,
            histogram,
            finished: false,
            _phantom: PhantomData,
        }
    }

    /// Run one round of the bench.
    ///
    /// If `finish` is true, the shared resources of the target (connection pools, etc.) will be
    /// released before the summary is generated, and the runner can not be run again.
    ///
    /// The runtime stats and histograms are accumulated across runs.
    pub(super) async fn run(&mut self, finish: bool) -> anyhow::Result<ExitCode> {
        if self.finished {
            return Err(anyhow!(
                "bench runner for {} has finished",
                self.target_name
            ));
        }

        let concurrency = self.proc_args.concurrency.get();
        stats::reset_global_state(self.proc_args.requests, self.proc_args.log_error_count);

        let mut contexts = Vec::with_capacity(concurrency);
        for i in 0..concurrency {
            let context = self
                .target
                .new_context()
                .context(format!("failed to to create context #{i}"))?;
            contexts.push(context);
        }

        let progress = self.proc_args.new_progress_bar();
        let task_config = TaskLoopConfig {
            task_unconstrained: self.proc_args.task_unconstrained,
            latency: self.proc_args.latency,
            ignore_fatal_error: self.proc_args.ignore_fatal_error,
            rate_limit: self
                .proc_args
                .rate_limit
                .map(|q| Arc::new(RateLimiter::new_global(q))),
            progress_counter: progress.as_ref().map(|p| p.counter()),
        };

        let mut monitors = Monitors::new();
        if let Some(progress) = progress {
            monitors.progress_bar = Some(progress.spawn(monitors.quit.clone())?);
        }
        monitors.runtime_stats = self.spawn_runtime_stats(&monitors.quit)?;
        monitors.histogram = self.spawn_histogram(&monitors.quit)?;

        let sync_sem = Arc::new(Semaphore::new(0));
        let sync_barrier = Arc::new(Barrier::new(concurrency + 1));
        let (sender, mut receiver) = mpsc::channel::<usize>(concurrency);
        for (i, context) in contexts.into_iter().enumerate() {
            let task_loop = TaskLoop {
                index: i,
                config: task_config.clone(),
                sync_sem: Arc::clone(&sync_sem),
                sync_barrier: Arc::clone(&sync_barrier),
                quit_sender: sender.clone(),
            };
            let rt =
                crate::worker::select_handle(i).unwrap_or_else(tokio::runtime::Handle::current);
            rt.spawn(task_loop.run(context));
        }
        drop(sender);

        let _run_permit = sync_sem
            .acquire_many(concurrency as u32)
            .await
            .context("failed to start all task contexts")?;

        let time_start = Instant::now();
        sync_barrier.wait().await;

        let _quit_timer = match self.proc_args.time_limit {
            Some(time_limit) => Some(QuitTimer::spawn(time_limit).inspect_err(|_| {
                stats::mark_time_up();
            })?),
            None => None,
        };

        let mut distribute_histogram = Histogram::<u64>::new(3).unwrap();
        while let Some(req_count) = receiver.recv().await {
            distribute_histogram.record(req_count as u64).unwrap();
        }
        let total_time = time_start.elapsed();

        monitors.stop();
        if finish {
            self.finished = true;
            self.target.notify_finish();
        }
        let histogram = monitors.join_histogram();

        let distribution = (concurrency > 1).then_some(&distribute_histogram);
        self.report(total_time, distribution, histogram)?;

        let exit_code = if stats::global_state().all_succeeded() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
        Ok(exit_code)
    }

    fn spawn_runtime_stats(
        &self,
        quit: &Arc<AtomicBool>,
    ) -> anyhow::Result<Option<JoinHandle<()>>> {
        let Some((mut statsd_client, emit_interval)) = self.proc_args.new_statsd_client() else {
            return Ok(None);
        };

        let runtime_stats = self.target.fetch_runtime_stats();
        let quit = quit.clone();
        let handler = std::thread::Builder::new()
            .name("runtime-stats".into())
            .spawn(move || {
                loop {
                    runtime_stats.emit(&mut statsd_client);
                    statsd_client.flush_sink();

                    if quit.load(Ordering::Relaxed) {
                        break;
                    }

                    std::thread::sleep(emit_interval);
                }
            })
            .map_err(|e| anyhow!("failed to create runtime stats thread: {e}"))?;
        Ok(Some(handler))
    }

    fn spawn_histogram(&mut self, quit: &Arc<AtomicBool>) -> anyhow::Result<Option<JoinHandle<H>>> {
        let Some(mut histogram) = self.histogram.take() else {
            return Ok(None);
        };

        let quit = quit.clone();
        let thread_builder = std::thread::Builder::new().name("histogram".into());
        let handler =
            if let Some((mut statsd_client, emit_interval)) = self.proc_args.new_statsd_client() {
                thread_builder
                    .spawn(move || {
                        loop {
                            histogram.refresh();
                            histogram.emit(&mut statsd_client);

                            if quit.load(Ordering::Relaxed) {
                                break;
                            }

                            std::thread::sleep(emit_interval);
                        }
                        histogram
                    })
                    .map_err(|e| anyhow!("failed to create histogram metrics thread: {e}"))?
            } else {
                thread_builder
                    .spawn(move || {
                        loop {
                            histogram.refresh();

                            if quit.load(Ordering::Relaxed) {
                                break;
                            }

                            std::thread::sleep(Duration::from_millis(100));
                        }
                        histogram
                    })
                    .map_err(|e| anyhow!("failed to create histogram refresh thread: {e}"))?
            };
        Ok(Some(handler))
    }

    fn report(
        &mut self,
        total_time: Duration,
        distribution: Option<&Histogram<u64>>,
        histogram: Option<H>,
    ) -> anyhow::Result<()> {
        let proc_args = &self.proc_args;
        if !proc_args.no_summary {
            stats::global_state().summary(total_time, distribution);
            self.target.fetch_runtime_stats().summary(total_time);
        }

        let mut histogram_json = report::JsonObject::new();
        if let Some(mut histogram) = histogram {
            histogram.refresh();
            if !proc_args.no_summary {
                histogram.summary();
            }
            if proc_args.json_file.is_some() {
                histogram_json = histogram.json_report();
            }
            self.histogram = Some(histogram);
        }

        let Some(path) = &proc_args.json_file else {
            return Ok(());
        };

        let mut root = report::JsonObject::new();
        report::insert(&mut root, report::keys::VERSION, Value::Number(1.into()));
        report::insert(
            &mut root,
            report::keys::TARGET,
            Value::String(self.target_name.to_string()),
        );
        report::insert(
            &mut root,
            report::keys::CONCURRENCY,
            report::json_usize(proc_args.concurrency.get()),
        );
        report::insert(
            &mut root,
            report::keys::GLOBAL,
            stats::global_state().json_report(total_time, distribution),
        );

        for (key, value) in self.target.fetch_runtime_stats().json_report(total_time) {
            root.insert(key, value);
        }
        for (key, value) in histogram_json {
            root.insert(key, value);
        }

        report::write_json_file(path, &Value::Object(root))
    }
}

fn register_signal_handler() {
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            eprintln!("error when waiting Ctrl-C: {e}");
        }
        stats::mark_force_quit();
    });
}

#[derive(Clone)]
struct TaskLoopConfig {
    task_unconstrained: bool,
    latency: Option<Duration>,
    ignore_fatal_error: bool,
    rate_limit: Option<Arc<RateLimiter<GlobalRateLimitState>>>,
    progress_counter: Option<ProgressCounter>,
}

struct TaskLoop {
    index: usize,
    config: TaskLoopConfig,
    sync_sem: Arc<Semaphore>,
    sync_barrier: Arc<Barrier>,
    quit_sender: mpsc::Sender<usize>,
}

impl TaskLoop {
    async fn run<C: BenchTaskContext>(self, mut context: C) {
        self.sync_sem.add_permits(1);
        self.sync_barrier.wait().await;

        let config = &self.config;
        let mut latency_interval = config.latency.map(|latency| {
            let mut interval = tokio::time::interval(latency);
            interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
            interval
        });

        let global_state = stats::global_state();
        let mut req_count = 0;
        while let Some(task_id) = global_state.fetch_request() {
            if let Some(latency) = &mut latency_interval {
                latency.tick().await;
            }

            if let Some(r) = &config.rate_limit {
                while let Err(t) = r.check() {
                    tokio::time::sleep(t).await;
                }
            }

            let time_start = Instant::now();
            context.mark_task_start();
            let rt = if config.task_unconstrained {
                tokio::task::unconstrained(context.run(task_id, time_start)).await
            } else {
                context.run(task_id, time_start).await
            };
            match rt {
                Ok(_) => {
                    context.mark_task_passed();
                    if let Some(c) = config.progress_counter.as_ref() {
                        c.inc();
                    }
                    global_state.add_passed();
                }
                Err(BenchError::Fatal(e)) => {
                    context.mark_task_failed();
                    global_state.add_failed();
                    if config.ignore_fatal_error {
                        if global_state.check_log_error() {
                            eprintln!("! request {task_id} failed: {e:?}\n");
                        }
                    } else {
                        eprintln!("!! Fatal error with task context {}: {e:?}", self.index);
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
        if let Err(e) = self.quit_sender.send(req_count).await {
            eprintln!("failed to send quit signal: {e}");
        }
    }
}

/// Background threads that run alongside the tasks.
///
/// They are told to quit on drop, so an early return won't leave them running.
struct Monitors<H> {
    quit: Arc<AtomicBool>,
    progress_bar: Option<JoinHandle<BenchProgress>>,
    runtime_stats: Option<JoinHandle<()>>,
    histogram: Option<JoinHandle<H>>,
}

impl<H> Monitors<H> {
    fn new() -> Self {
        Monitors {
            quit: Arc::new(AtomicBool::new(false)),
            progress_bar: None,
            runtime_stats: None,
            histogram: None,
        }
    }

    fn stop(&mut self) {
        self.quit.store(true, Ordering::Relaxed);

        if let Some(handler) = self.progress_bar.take() {
            match handler.join() {
                Ok(bar) => bar.finish(),
                Err(e) => eprintln!("error to join progress bar thread: {e:?}"),
            }
        }

        if let Some(handler) = self.runtime_stats.take() {
            let _ = handler.join();
        }
    }

    fn join_histogram(&mut self) -> Option<H> {
        let handler = self.histogram.take()?;
        match handler.join() {
            Ok(histogram) => Some(histogram),
            Err(e) => {
                eprintln!("error to join histogram stats thread: {e:?}");
                None
            }
        }
    }
}

impl<H> Drop for Monitors<H> {
    fn drop(&mut self) {
        self.quit.store(true, Ordering::Relaxed);
    }
}

/// Marks the current run as time up after the time limit, unless dropped before that.
struct QuitTimer {
    _cancel: std_mpsc::Sender<()>,
}

impl QuitTimer {
    fn spawn(time_limit: Duration) -> anyhow::Result<Self> {
        let (cancel, cancel_receiver) = std_mpsc::channel::<()>();
        std::thread::Builder::new()
            .name("quit-timer".into())
            .spawn(move || {
                if let Err(std_mpsc::RecvTimeoutError::Timeout) =
                    cancel_receiver.recv_timeout(time_limit)
                {
                    stats::mark_time_up();
                }
            })
            .map_err(|e| anyhow!("failed to create quit timer thread: {e}"))?;
        Ok(QuitTimer { _cancel: cancel })
    }
}
