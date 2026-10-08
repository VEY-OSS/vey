/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use ahash::AHashMap;
use anyhow::anyhow;
use http::uri::PathAndQuery;
use http::{HeaderMap, HeaderName, HeaderValue, header};
use jiff::Timestamp;
use tokio::sync::mpsc;

use vey_http::client::HttpForwardRemoteResponse;
use vey_types::metrics::{MetricTagMap, MetricTagName, MetricTagValue};

use crate::config::exporter::prometheus_push::PrometheusPushExporterConfig;
use crate::runtime::export::{AggregateExport, CounterStoreValue, GaugeStoreValue, HttpExport};
use crate::types::MetricName;

const PROMETHEUS_NAME_LABEL: MetricTagName =
    unsafe { MetricTagName::new_static_unchecked("__name__") };

pub(super) struct PrometheusEncodedBatch {
    samples: usize,
    buf: Vec<u8>,
}

pub(super) struct PrometheusAggregateExport {
    emit_interval: Duration,
    max_samples: usize,
    prefix: Option<MetricName>,
    sanitized_global_labels: BTreeMap<MetricTagName, MetricTagValue>,
    batch_sender: mpsc::UnboundedSender<PrometheusEncodedBatch>,

    batch_buf: Vec<u8>,
    samples: usize,
    series_buf: Vec<u8>,
    small_encode_buf: Vec<u8>,
    joined_metric_name: String,
    prometheus_metric_name: String,
    sanitized_label_name: String,
    series_labels: BTreeMap<MetricTagName, MetricTagValue>,
}

impl PrometheusAggregateExport {
    pub(super) fn new(
        config: &PrometheusPushExporterConfig,
        batch_sender: mpsc::UnboundedSender<PrometheusEncodedBatch>,
    ) -> Self {
        let mut export = PrometheusAggregateExport {
            emit_interval: config.emit_interval,
            max_samples: config.max_samples,
            prefix: config.prefix.clone(),
            sanitized_global_labels: BTreeMap::new(),
            batch_sender,
            batch_buf: Vec::with_capacity(2048),
            samples: 0,
            series_buf: Vec::with_capacity(256),
            small_encode_buf: Vec::with_capacity(64),
            joined_metric_name: String::new(),
            prometheus_metric_name: String::new(),
            sanitized_label_name: String::new(),
            series_labels: BTreeMap::new(),
        };
        export.fill_sanitized_global_labels(&config.global_tags);
        export
    }

    fn fill_sanitized_global_labels(&mut self, tags: &MetricTagMap) {
        for (name, value) in tags.iter() {
            let key = self.sanitize_label_name(name);
            self.sanitized_global_labels.insert(key, value.clone());
        }
    }

    fn write_prometheus_metric_name(&mut self, name: &MetricName) {
        use std::fmt::Write;
        self.joined_metric_name.clear();
        if let Some(prefix) = &self.prefix {
            let _ = write!(&mut self.joined_metric_name, "{}", prefix.display('_'));
            self.joined_metric_name.push('_');
        }
        let _ = write!(&mut self.joined_metric_name, "{}", name.display('_'));

        let Self {
            joined_metric_name,
            prometheus_metric_name,
            ..
        } = self;
        push_sanitized_metric(joined_metric_name, prometheus_metric_name);
    }

    fn sanitize_label_name(&mut self, name: &MetricTagName) -> MetricTagName {
        self.sanitized_label_name.clear();
        push_sanitized_metric(name.as_str(), &mut self.sanitized_label_name);
        self.sanitized_label_name.retain(|c| c != ':');
        if self.sanitized_label_name.is_empty()
            || self.sanitized_label_name.as_bytes()[0].is_ascii_digit()
        {
            self.sanitized_label_name.insert(0, '_');
        }
        if self.sanitized_label_name.starts_with("__") {
            self.sanitized_label_name.insert_str(0, "key_");
        }
        MetricTagName::from_str(&self.sanitized_label_name).unwrap_or_else(|_| name.clone())
    }

    fn fill_series_labels(&mut self, tags: &MetricTagMap) {
        for (name, value) in &self.sanitized_global_labels {
            self.series_labels.insert(name.clone(), value.clone());
        }
        for (name, value) in tags.iter() {
            let key = self.sanitize_label_name(name);
            self.series_labels.insert(key, value.clone());
        }
        self.series_labels
            .insert(PROMETHEUS_NAME_LABEL.clone(), MetricTagValue::EMPTY);
    }

    fn put_sample(&mut self, value: f64, ts_ms: i64) {
        self.small_encode_buf.clear();
        put_fixed64(&mut self.small_encode_buf, 1, value.to_bits());
        put_varint_field(&mut self.small_encode_buf, 2, ts_ms as u64);
        put_bytes(&mut self.series_buf, 2, &self.small_encode_buf);
    }

    fn append_labels(&mut self) {
        let Self {
            series_labels,
            small_encode_buf,
            series_buf,
            prometheus_metric_name,
            ..
        } = self;
        for (name, value) in series_labels {
            small_encode_buf.clear();
            put_string(small_encode_buf, 1, name.as_str());
            if value.as_str().is_empty() && name.as_str() == "__name__" {
                put_string(small_encode_buf, 2, prometheus_metric_name);
            } else {
                put_string(small_encode_buf, 2, value.as_str());
            }
            put_bytes(series_buf, 1, small_encode_buf);
        }
    }

    fn append_timeseries(&mut self, value: f64, ts_ms: i64) {
        self.append_labels();
        self.put_sample(value, ts_ms);
        put_bytes(&mut self.batch_buf, 1, &self.series_buf);
        self.samples += 1;
        if self.samples >= self.max_samples {
            self.flush();
        }
    }

    fn emit_value(&mut self, tags: &MetricTagMap, value: f64, time: Timestamp) {
        if !value.is_finite() {
            return;
        }
        self.series_labels.clear();
        self.fill_series_labels(tags);
        self.series_buf.clear();
        self.append_timeseries(value, time.as_millisecond());
    }

    fn flush(&mut self) {
        if self.samples == 0 {
            return;
        }
        let new_buf = Vec::with_capacity(self.batch_buf.capacity());
        let buf = std::mem::replace(&mut self.batch_buf, new_buf);
        let samples = self.samples;
        self.samples = 0;
        let _ = self
            .batch_sender
            .send(PrometheusEncodedBatch { samples, buf });
    }
}

impl AggregateExport for PrometheusAggregateExport {
    fn emit_interval(&self) -> Duration {
        self.emit_interval
    }

    fn emit_gauge(
        &mut self,
        name: &MetricName,
        values: &AHashMap<Arc<MetricTagMap>, GaugeStoreValue>,
    ) {
        self.batch_buf.clear();
        self.samples = 0;
        self.prometheus_metric_name.clear();
        self.write_prometheus_metric_name(name);
        for (tags, gauge) in values {
            self.emit_value(tags, gauge.value.as_f64(), gauge.time);
        }
        self.flush();
    }

    fn emit_counter(
        &mut self,
        name: &MetricName,
        values: &AHashMap<Arc<MetricTagMap>, CounterStoreValue>,
    ) {
        self.batch_buf.clear();
        self.samples = 0;
        self.prometheus_metric_name.clear();
        self.write_prometheus_metric_name(name);
        for (tags, counter) in values {
            self.emit_value(tags, counter.sum.as_f64(), counter.time);
        }
        self.flush();
    }
}

pub(super) struct PrometheusHttpExport {
    api_path: PathAndQuery,
    static_headers: HeaderMap,
    max_samples: usize,
    raw_buf: Vec<u8>,
    encoder: snap::raw::Encoder,
}

impl PrometheusHttpExport {
    pub(super) fn new(config: &PrometheusPushExporterConfig) -> anyhow::Result<Self> {
        let api_path = config.build_api_path()?;
        let mut static_headers = HeaderMap::new();
        static_headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-protobuf"),
        );
        static_headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("snappy"));
        static_headers.insert(
            HeaderName::from_static("x-prometheus-remote-write-version"),
            HeaderValue::from_static("0.1.0"),
        );
        if !config.bearer_token.is_empty() {
            let value = HeaderValue::from_str(&format!("Bearer {}", config.bearer_token))
                .map_err(|e| anyhow!("invalid bearer token: {e}"))?;
            static_headers.insert(header::AUTHORIZATION, value);
        }
        if !config.org_id.is_empty() {
            let value = HeaderValue::from_str(&config.org_id)
                .map_err(|e| anyhow!("invalid org_id: {e}"))?;
            static_headers.insert(HeaderName::from_static("x-scope-orgid"), value);
        }
        Ok(PrometheusHttpExport {
            api_path,
            static_headers,
            max_samples: config.max_samples,
            raw_buf: Vec::with_capacity(2048),
            encoder: snap::raw::Encoder::new(),
        })
    }

    fn compress_block(&mut self, dst: &mut Vec<u8>) -> Result<(), snap::Error> {
        let max_len = snap::raw::max_compress_len(self.raw_buf.len());
        dst.resize(max_len, 0);
        let n = self.encoder.compress(&self.raw_buf, dst)?;
        dst.truncate(n);
        Ok(())
    }

    fn extend_raw_body(&mut self, pieces: &[PrometheusEncodedBatch]) -> usize {
        let mut samples = 0;
        let mut handled = 0;
        for piece in pieces {
            if samples > 0 && samples + piece.samples > self.max_samples {
                break;
            }
            self.raw_buf.extend_from_slice(&piece.buf);
            samples += piece.samples;
            handled += 1;
        }
        handled
    }
}

// https://prometheus.io/docs/specs/prw/remote_write_spec/
impl HttpExport for PrometheusHttpExport {
    type BodyPiece = PrometheusEncodedBatch;

    fn api_path(&self) -> &PathAndQuery {
        &self.api_path
    }

    fn static_headers(&self) -> &HeaderMap {
        &self.static_headers
    }

    fn fill_body(&mut self, pieces: &[PrometheusEncodedBatch], body_buf: &mut Vec<u8>) -> usize {
        self.raw_buf.clear();
        let handled = self.extend_raw_body(pieces);
        if handled == 0 {
            return 0;
        }
        match self.compress_block(body_buf) {
            Ok(()) => handled,
            Err(_) => {
                body_buf.clear();
                0
            }
        }
    }

    fn check_response(&self, rsp: HttpForwardRemoteResponse, body: &[u8]) -> anyhow::Result<()> {
        if (200..300).contains(&rsp.code) {
            Ok(())
        } else if let Ok(detail) = std::str::from_utf8(body) {
            Err(anyhow!("error response: {} {detail}", rsp.code))
        } else {
            Err(anyhow!("error response: {}", rsp.code))
        }
    }
}

fn push_sanitized_metric(name: &str, out: &mut String) {
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == ':' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() || out.as_bytes()[0].is_ascii_digit() {
        out.insert(0, '_');
    }
}

fn put_varint(buf: &mut Vec<u8>, mut n: u64) {
    loop {
        let mut b = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            b |= 0x80;
        }
        buf.push(b);
        if n == 0 {
            break;
        }
    }
}

fn put_varint_field(buf: &mut Vec<u8>, field: u32, value: u64) {
    put_varint(buf, (field as u64) << 3);
    put_varint(buf, value);
}

fn put_fixed64(buf: &mut Vec<u8>, field: u32, value: u64) {
    put_varint(buf, ((field as u64) << 3) | 1);
    buf.extend_from_slice(&value.to_le_bytes());
}

fn put_bytes(buf: &mut Vec<u8>, field: u32, bytes: &[u8]) {
    put_varint(buf, ((field as u64) << 3) | 2);
    put_varint(buf, bytes.len() as u64);
    buf.extend_from_slice(bytes);
}

fn put_string(buf: &mut Vec<u8>, field: u32, value: &str) {
    put_bytes(buf, field, value.as_bytes());
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use yaml_rust::YamlLoader;

    use vey_types::metrics::{MetricTagName, MetricTagValue};

    use crate::config::exporter::prometheus_push::PrometheusPushExporterConfig;
    use crate::types::MetricValue;

    fn export(
        yaml: &str,
    ) -> (
        PrometheusAggregateExport,
        mpsc::UnboundedReceiver<PrometheusEncodedBatch>,
    ) {
        let docs = YamlLoader::load_from_str(yaml).unwrap();
        let cfg = PrometheusPushExporterConfig::parse(docs[0].as_hash().unwrap(), None).unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        (PrometheusAggregateExport::new(&cfg, tx), rx)
    }

    fn read_varint(buf: &[u8], i: &mut usize) -> u64 {
        let mut n = 0u64;
        let mut shift = 0;
        loop {
            let b = buf[*i];
            *i += 1;
            n |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return n;
            }
            shift += 7;
        }
    }

    fn read_len<'a>(buf: &'a [u8], i: &mut usize) -> &'a [u8] {
        let len = read_varint(buf, i) as usize;
        let start = *i;
        *i += len;
        &buf[start..*i]
    }

    struct DecodedSample {
        labels: Vec<(String, String)>,
        value: f64,
        timestamp_ms: i64,
    }

    fn decode_batch(buf: &[u8]) -> Vec<DecodedSample> {
        let mut i = 0;
        let mut samples = Vec::new();
        while i < buf.len() {
            let key = read_varint(buf, &mut i);
            assert_eq!(key, (1 << 3) | 2);
            let series = read_len(buf, &mut i);
            samples.push(decode_series(series));
        }
        samples
    }

    fn decode_series(buf: &[u8]) -> DecodedSample {
        let mut i = 0;
        let mut labels = Vec::new();
        let mut value = None;
        let mut timestamp_ms = None;
        while i < buf.len() {
            let key = read_varint(buf, &mut i);
            let field = key >> 3;
            let wire = key & 0x7;
            match (field, wire) {
                (1, 2) => {
                    let label = read_len(buf, &mut i);
                    labels.push(decode_label(label));
                }
                (2, 2) => {
                    let sample = read_len(buf, &mut i);
                    let (sample_value, ts) = decode_sample(sample);
                    value = Some(sample_value);
                    timestamp_ms = Some(ts);
                }
                _ => panic!("unexpected series field {field} wire {wire}"),
            }
        }
        DecodedSample {
            labels,
            value: value.unwrap(),
            timestamp_ms: timestamp_ms.unwrap(),
        }
    }

    fn label_value(labels: &[(String, String)], name: &str) -> String {
        labels
            .iter()
            .find(|(label, _)| label == name)
            .unwrap()
            .1
            .clone()
    }

    fn decode_label(buf: &[u8]) -> (String, String) {
        let mut i = 0;
        let mut name = None;
        let mut value = None;
        while i < buf.len() {
            let key = read_varint(buf, &mut i);
            let field = key >> 3;
            assert_eq!(key & 0x7, 2);
            let bytes = read_len(buf, &mut i);
            let text = std::str::from_utf8(bytes).unwrap().to_string();
            match field {
                1 => name = Some(text),
                2 => value = Some(text),
                _ => panic!("unexpected label field {field}"),
            }
        }
        (name.unwrap(), value.unwrap())
    }

    fn decode_sample(buf: &[u8]) -> (f64, i64) {
        let mut i = 0;
        let mut value = None;
        let mut timestamp_ms = None;
        while i < buf.len() {
            let key = read_varint(buf, &mut i);
            let field = key >> 3;
            match (field, key & 0x7) {
                (1, 1) => {
                    let mut bits = [0u8; 8];
                    bits.copy_from_slice(&buf[i..i + 8]);
                    i += 8;
                    value = Some(f64::from_bits(u64::from_le_bytes(bits)));
                }
                (2, 0) => timestamp_ms = Some(read_varint(buf, &mut i) as i64),
                _ => panic!("unexpected sample field {field}"),
            }
        }
        (value.unwrap(), timestamp_ms.unwrap())
    }

    #[test]
    fn emit_counter_writes_sum() {
        let name = MetricName::parse("foo.bar").unwrap();
        let tags = Arc::new(MetricTagMap::default());
        let time = "2020-01-02T03:04:05Z".parse::<Timestamp>().unwrap();
        let mut values = AHashMap::new();
        values.insert(
            tags,
            CounterStoreValue {
                time,
                sum: MetricValue::Unsigned(100),
                diff: MetricValue::Unsigned(7),
            },
        );

        let (mut export, mut rx) = export(
            r#"
name: p1
server: 127.0.0.1
emit_interval: 10s
"#,
        );
        export.emit_counter(&name, &values);
        let batch = rx.try_recv().unwrap();
        assert!(rx.try_recv().is_err());
        assert_eq!(batch.samples, 1);

        let samples = decode_batch(&batch.buf);
        assert_eq!(samples.len(), 1);
        assert_eq!(label_value(&samples[0].labels, "__name__"), "foo_bar");
        assert_eq!(samples[0].value, 100.0);
        assert_eq!(samples[0].timestamp_ms, time.as_millisecond());
    }

    #[test]
    fn folds_names_and_sorts_labels() {
        let name = MetricName::parse("9xx.hits").unwrap();
        let mut tags = MetricTagMap::default();
        tags.insert(
            MetricTagName::from_str("b-host").unwrap(),
            MetricTagValue::from_str("web").unwrap(),
        );
        tags.insert(
            MetricTagName::from_str("__role").unwrap(),
            MetricTagValue::from_str("edge").unwrap(),
        );
        tags.insert(
            MetricTagName::from_str("0host").unwrap(),
            MetricTagValue::from_str("n1").unwrap(),
        );
        let time = Timestamp::from_millisecond(1_700_000_000_000).unwrap();
        let mut values = AHashMap::new();
        values.insert(
            Arc::new(tags),
            GaugeStoreValue {
                time,
                value: MetricValue::Double(3.5),
            },
        );

        let (mut export, mut rx) = export(
            r#"
name: p1
server: 127.0.0.1
prefix: app.web
global_tags:
  zone: a
"#,
        );
        export.emit_gauge(&name, &values);
        let batch = rx.try_recv().unwrap();
        let sample = &decode_batch(&batch.buf)[0];
        assert_eq!(label_value(&sample.labels, "__name__"), "app_web_9xx_hits");
        assert_eq!(label_value(&sample.labels, "_0host"), "n1");
        assert_eq!(label_value(&sample.labels, "b_host"), "web");
        assert_eq!(label_value(&sample.labels, "key___role"), "edge");
        assert_eq!(label_value(&sample.labels, "zone"), "a");
        let names: Vec<&str> = sample
            .labels
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
        assert_eq!(sample.value, 3.5);
    }

    #[test]
    fn collapsed_label_names_keep_one_value() {
        let name = MetricName::parse("c").unwrap();
        let mut tags = MetricTagMap::default();
        tags.insert(
            MetricTagName::from_str("a-b").unwrap(),
            MetricTagValue::from_str("1").unwrap(),
        );
        tags.insert(
            MetricTagName::from_str("a_b").unwrap(),
            MetricTagValue::from_str("2").unwrap(),
        );
        let mut values = AHashMap::new();
        values.insert(
            Arc::new(tags),
            CounterStoreValue {
                time: Timestamp::from_second(10).unwrap(),
                sum: MetricValue::Unsigned(1),
                diff: MetricValue::Unsigned(1),
            },
        );

        let (mut export, mut rx) = export(
            r#"
name: p1
server: 127.0.0.1
"#,
        );
        export.emit_counter(&name, &values);
        let sample = &decode_batch(&rx.try_recv().unwrap().buf)[0];
        assert_eq!(label_value(&sample.labels, "a_b"), "2");
        assert_eq!(sample.labels.len(), 2);
    }

    #[test]
    fn splits_batches_at_max_samples() {
        let name = MetricName::parse("c").unwrap();
        let mut values = AHashMap::new();
        for (label, sum) in [("a", 1u64), ("b", 2)] {
            let mut tags = MetricTagMap::default();
            tags.insert(
                MetricTagName::from_str("k").unwrap(),
                MetricTagValue::from_str(label).unwrap(),
            );
            values.insert(
                Arc::new(tags),
                CounterStoreValue {
                    time: Timestamp::from_second(10).unwrap(),
                    sum: MetricValue::Unsigned(sum),
                    diff: MetricValue::Unsigned(sum),
                },
            );
        }

        let (mut export, mut rx) = export(
            r#"
name: p1
server: 127.0.0.1
max_samples: 1
"#,
        );
        export.emit_counter(&name, &values);
        let first = rx.try_recv().unwrap();
        let second = rx.try_recv().unwrap();
        assert!(rx.try_recv().is_err());
        assert_eq!(first.samples, 1);
        assert_eq!(second.samples, 1);
    }

    #[test]
    fn http_body_is_snappy_block() {
        let docs = YamlLoader::load_from_str(
            r#"
name: p1
server: 127.0.0.1
bearer_token: secret
org_id: tenant-a
path: /api/v1/push
"#,
        )
        .unwrap();
        let cfg = PrometheusPushExporterConfig::parse(docs[0].as_hash().unwrap(), None).unwrap();
        let mut http = PrometheusHttpExport::new(&cfg).unwrap();
        assert_eq!(http.api_path().as_str(), "/api/v1/push");
        assert_eq!(
            http.static_headers().get(header::CONTENT_TYPE).unwrap(),
            "application/x-protobuf"
        );
        assert_eq!(
            http.static_headers().get(header::CONTENT_ENCODING).unwrap(),
            "snappy"
        );
        assert_eq!(
            http.static_headers()
                .get("x-prometheus-remote-write-version")
                .unwrap(),
            "0.1.0"
        );
        assert_eq!(
            http.static_headers().get(header::AUTHORIZATION).unwrap(),
            "Bearer secret"
        );
        assert_eq!(
            http.static_headers().get("x-scope-orgid").unwrap(),
            "tenant-a"
        );

        let raw = b"\x0a\x02hi".to_vec();
        let pieces = [PrometheusEncodedBatch {
            samples: 1,
            buf: raw.clone(),
        }];
        let mut body = Vec::new();
        assert_eq!(http.fill_body(&pieces, &mut body), 1);
        let out = snap::raw::Decoder::new().decompress_vec(&body).unwrap();
        assert_eq!(out, raw);
    }
}
