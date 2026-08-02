/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use anyhow::Context;
use hdrhistogram::Histogram;
use serde_json::{Map, Number, Value};

pub(crate) type JsonObject = Map<String, Value>;

/// Shared JSON field names used across targets.
pub(crate) mod keys {
    // root
    pub const VERSION: &str = "version";
    pub const TARGET: &str = "target";
    pub const CONCURRENCY: &str = "concurrency";
    pub const GLOBAL: &str = "global";
    pub const CONNECTIONS: &str = "connections";
    pub const TLS: &str = "tls";
    pub const TRAFFIC: &str = "traffic";
    pub const HISTOGRAMS: &str = "histograms";
    pub const PERCENTILES_NS: &str = "percentiles_ns";

    // global
    pub const TOTAL_TIME_NS: &str = "total_time_ns";
    pub const COMPLETE_REQUESTS: &str = "complete_requests";
    pub const FAILED_REQUESTS: &str = "failed_requests";
    pub const LEFT_REQUESTS: &str = "left_requests";
    pub const REQUESTS_PER_SEC: &str = "requests_per_sec";
    pub const REQUESTS_DISTRIBUTION: &str = "requests_distribution";

    // connections
    pub const ATTEMPT: &str = "attempt";
    pub const SUCCESS: &str = "success";
    pub const SUCCESS_RATIO: &str = "success_ratio";
    pub const SUCCESS_PER_SEC: &str = "success_per_sec";
    pub const CLOSE_ERROR: &str = "close_error";
    pub const CLOSE_TIMEOUT: &str = "close_timeout";

    // tls
    pub const PROXY: &str = "proxy";
    pub const TOTAL: &str = "total";
    pub const REUSED: &str = "reused";
    pub const REUSE_RATIO: &str = "reuse_ratio";

    // traffic (nested by protocol; multiple may be present at once)
    pub const TCP: &str = "tcp";
    pub const UDP: &str = "udp";
    pub const SEND_BYTES: &str = "send_bytes";
    pub const RECV_BYTES: &str = "recv_bytes";
    pub const SEND_PACKETS: &str = "send_packets";
    pub const RECV_PACKETS: &str = "recv_packets";
    pub const SEND_BPS: &str = "send_bps";
    pub const RECV_BPS: &str = "recv_bps";
    pub const SEND_PPS: &str = "send_pps";
    pub const RECV_PPS: &str = "recv_pps";

    // histograms
    pub const CONN_USED_TIMES: &str = "conn_used_times";
    pub const DURATIONS_NS: &str = "durations_ns";
    pub const CONNECT: &str = "connect";
    pub const SEND_HDR: &str = "send_hdr";
    pub const SEND_ALL: &str = "send_all";
    pub const RECV_HDR: &str = "recv_hdr";

    // hist snapshot / percentiles
    pub const MIN: &str = "min";
    pub const MEAN: &str = "mean";
    pub const STDEV: &str = "stdev";
    pub const P90: &str = "p90";
    pub const MAX: &str = "max";
    pub const P50: &str = "p50";
    pub const P66: &str = "p66";
    pub const P75: &str = "p75";
    pub const P80: &str = "p80";
    pub const P95: &str = "p95";
    pub const P98: &str = "p98";
    pub const P99: &str = "p99";
    pub const P100: &str = "p100";
}

pub(crate) fn insert(obj: &mut JsonObject, key: &str, value: Value) {
    obj.insert(key.to_string(), value);
}

pub(crate) fn json_u64(v: u64) -> Value {
    Value::Number(Number::from(v))
}

pub(crate) fn json_usize(v: usize) -> Value {
    Value::Number(Number::from(v))
}

pub(crate) fn json_f64(v: f64) -> Value {
    Number::from_f64(v)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

pub(crate) fn hist_snapshot(h: &Histogram<u64>) -> Value {
    let mut obj = JsonObject::new();
    insert(&mut obj, keys::MIN, json_u64(h.min()));
    insert(&mut obj, keys::MEAN, json_f64(h.mean()));
    insert(&mut obj, keys::STDEV, json_f64(h.stdev()));
    insert(&mut obj, keys::P90, json_u64(h.value_at_quantile(0.90)));
    insert(&mut obj, keys::MAX, json_u64(h.max()));
    Value::Object(obj)
}

pub(crate) fn percentiles_ns(h: &Histogram<u64>) -> Option<Value> {
    if h.len() <= 1 {
        return None;
    }

    let mut obj = JsonObject::new();
    insert(&mut obj, keys::P50, json_u64(h.value_at_percentile(50.0)));
    insert(&mut obj, keys::P66, json_u64(h.value_at_percentile(66.0)));
    insert(&mut obj, keys::P75, json_u64(h.value_at_percentile(75.0)));
    insert(&mut obj, keys::P80, json_u64(h.value_at_percentile(80.0)));
    insert(&mut obj, keys::P90, json_u64(h.value_at_percentile(90.0)));
    insert(&mut obj, keys::P95, json_u64(h.value_at_percentile(95.0)));
    insert(&mut obj, keys::P98, json_u64(h.value_at_percentile(98.0)));
    insert(&mut obj, keys::P99, json_u64(h.value_at_percentile(99.0)));
    insert(&mut obj, keys::P100, json_u64(h.value_at_percentile(100.0)));
    Some(Value::Object(obj))
}

pub(crate) fn connections_object(
    attempt: u64,
    success: u64,
    total_secs: f64,
    close_error: u64,
    close_timeout: u64,
) -> Value {
    let success_ratio = if attempt == 0 {
        0.0
    } else {
        success as f64 / attempt as f64
    };
    let mut obj = JsonObject::new();
    insert(&mut obj, keys::ATTEMPT, json_u64(attempt));
    insert(&mut obj, keys::SUCCESS, json_u64(success));
    insert(&mut obj, keys::SUCCESS_RATIO, json_f64(success_ratio));
    insert(
        &mut obj,
        keys::SUCCESS_PER_SEC,
        json_f64(success as f64 / total_secs),
    );
    insert(&mut obj, keys::CLOSE_ERROR, json_u64(close_error));
    insert(&mut obj, keys::CLOSE_TIMEOUT, json_u64(close_timeout));
    Value::Object(obj)
}

pub(crate) fn tls_session_object(total: u64, reused: u64) -> Option<Value> {
    if total == 0 {
        return None;
    }
    let mut obj = JsonObject::new();
    insert(&mut obj, keys::TOTAL, json_u64(total));
    insert(&mut obj, keys::REUSED, json_u64(reused));
    insert(
        &mut obj,
        keys::REUSE_RATIO,
        json_f64(reused as f64 / total as f64),
    );
    Some(Value::Object(obj))
}

pub(crate) fn tcp_traffic_stats(send_bytes: u64, recv_bytes: u64, total_secs: f64) -> Value {
    let mut obj = JsonObject::new();
    insert(&mut obj, keys::SEND_BYTES, json_u64(send_bytes));
    insert(&mut obj, keys::RECV_BYTES, json_u64(recv_bytes));
    insert(
        &mut obj,
        keys::SEND_BPS,
        json_f64(send_bytes as f64 / total_secs),
    );
    insert(
        &mut obj,
        keys::RECV_BPS,
        json_f64(recv_bytes as f64 / total_secs),
    );
    Value::Object(obj)
}

pub(crate) fn udp_traffic_stats(
    send_bytes: u64,
    send_packets: u64,
    recv_bytes: u64,
    recv_packets: u64,
    total_secs: f64,
) -> Value {
    let mut obj = JsonObject::new();
    insert(&mut obj, keys::SEND_BYTES, json_u64(send_bytes));
    insert(&mut obj, keys::SEND_PACKETS, json_u64(send_packets));
    insert(&mut obj, keys::RECV_BYTES, json_u64(recv_bytes));
    insert(&mut obj, keys::RECV_PACKETS, json_u64(recv_packets));
    insert(
        &mut obj,
        keys::SEND_BPS,
        json_f64(send_bytes as f64 / total_secs),
    );
    insert(
        &mut obj,
        keys::RECV_BPS,
        json_f64(recv_bytes as f64 / total_secs),
    );
    insert(
        &mut obj,
        keys::SEND_PPS,
        json_f64(send_packets as f64 / total_secs),
    );
    insert(
        &mut obj,
        keys::RECV_PPS,
        json_f64(recv_packets as f64 / total_secs),
    );
    Value::Object(obj)
}

pub(crate) fn write_json_file(path: &Path, value: &Value) -> anyhow::Result<()> {
    let file = File::create(path)
        .with_context(|| format!("failed to create json report file {}", path.display()))?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, value).context("failed to write json report")?;
    Ok(())
}
