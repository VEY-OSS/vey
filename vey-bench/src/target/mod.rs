/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use hdrhistogram::Histogram;
use tokio::time::Instant;

use vey_statsd_client::StatsdClient;

use super::ProcArgs;
use crate::report::JsonObject;

mod runner;
use runner::BenchRunner;

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

async fn run<RS, H, C, T>(
    target: T,
    proc_args: &Arc<ProcArgs>,
    target_name: &'static str,
) -> anyhow::Result<ExitCode>
where
    RS: BenchRuntimeStats + Send + Sync + 'static,
    H: BenchHistogram + Send + 'static,
    C: BenchTaskContext + Send + 'static,
    T: BenchTarget<RS, H, C> + Send + Sync + 'static,
{
    let mut runner = BenchRunner::new(target, proc_args, target_name);
    runner.run(true).await
}
