/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use ahash::AHashMap;
use chrono::{DateTime, Utc};
use itoa::Buffer;
use tokio::sync::mpsc;

use vey_types::metrics::MetricTagMap;

use crate::config::exporter::graphite::{GraphiteCounterValue, GraphiteExporterConfig};
use crate::runtime::export::{AggregateExport, CounterStoreValue, GaugeStoreValue, StreamExport};
use crate::types::{MetricName, MetricValue};

pub(super) struct GraphitePlaintextAggregateExport {
    emit_interval: Duration,
    prefix: Option<MetricName>,
    global_tags: MetricTagMap,
    counter_value: GraphiteCounterValue,
    data_sender: mpsc::UnboundedSender<Vec<u8>>,

    buf: Vec<u8>,
}

impl GraphitePlaintextAggregateExport {
    pub(super) fn new(
        config: &GraphiteExporterConfig,
        data_sender: mpsc::UnboundedSender<Vec<u8>>,
    ) -> Self {
        GraphitePlaintextAggregateExport {
            emit_interval: config.emit_interval,
            prefix: config.prefix.clone(),
            global_tags: config.global_tags.clone(),
            counter_value: config.counter_value,
            data_sender,
            buf: Vec::with_capacity(2048),
        }
    }

    fn serialize(
        &mut self,
        time: &DateTime<Utc>,
        name: &MetricName,
        tags: &MetricTagMap,
        value: &MetricValue,
    ) {
        if let Some(prefix) = &self.prefix {
            let _ = write!(self.buf, "{}.{}", prefix.display('.'), name.display('.'));
        } else {
            let _ = write!(self.buf, "{}", name.display('.'));
        }
        if !self.global_tags.is_empty() {
            let _ = write!(self.buf, ";{}", self.global_tags.display_graphite());
        }
        if !tags.is_empty() {
            let _ = write!(self.buf, ";{}", tags.display_graphite());
        }
        let _ = write!(self.buf, " {value}");
        let mut ts_buffer = Buffer::new();
        let ts = ts_buffer.format(time.timestamp());
        self.buf.push(b' ');
        self.buf.extend_from_slice(ts.as_bytes());
        self.buf.push(b'\n');
    }
}

impl AggregateExport for GraphitePlaintextAggregateExport {
    fn emit_interval(&self) -> Duration {
        self.emit_interval
    }

    fn emit_gauge(
        &mut self,
        name: &MetricName,
        values: &AHashMap<Arc<MetricTagMap>, GaugeStoreValue>,
    ) {
        self.buf.clear();
        let now = Utc::now();
        for (tags, v) in values {
            self.serialize(&now, name, tags, &v.value);
        }
        let _ = self.data_sender.send(self.buf.clone());
    }

    fn emit_counter(
        &mut self,
        name: &MetricName,
        values: &AHashMap<Arc<MetricTagMap>, CounterStoreValue>,
    ) {
        self.buf.clear();
        let now = Utc::now();
        for (tags, v) in values {
            let value = match self.counter_value {
                GraphiteCounterValue::Sum => &v.sum,
                GraphiteCounterValue::Diff => &v.diff,
            };
            self.serialize(&now, name, tags, value);
        }
        let _ = self.data_sender.send(self.buf.clone());
    }
}

#[derive(Default)]
pub(super) struct GraphitePlaintextStreamExport {}

impl StreamExport for GraphitePlaintextStreamExport {
    type Piece = Vec<u8>;

    fn serialize(&self, pieces: &[Vec<u8>], buf: &mut Vec<u8>) -> usize {
        for piece in pieces {
            buf.extend_from_slice(piece.as_slice());
        }
        pieces.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use yaml_rust::YamlLoader;

    use crate::config::exporter::graphite::GraphiteExporterConfig;

    fn export(
        yaml: &str,
    ) -> (
        GraphitePlaintextAggregateExport,
        mpsc::UnboundedReceiver<Vec<u8>>,
    ) {
        let docs = YamlLoader::load_from_str(yaml).unwrap();
        let cfg = GraphiteExporterConfig::parse(docs[0].as_hash().unwrap(), None).unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        (GraphitePlaintextAggregateExport::new(&cfg, tx), rx)
    }

    #[test]
    fn serialize_with_prefix_and_tags() {
        let (mut export, _) = export(
            r#"
name: g1
server: 127.0.0.1
prefix: pref
global_tags:
  env: prod
"#,
        );
        let time = Utc.with_ymd_and_hms(2020, 1, 2, 3, 4, 5).unwrap();
        let name = MetricName::parse("foo.bar").unwrap();
        let mut tags = MetricTagMap::default();
        tags.parse_statsd(b"k:v").unwrap();
        export.serialize(&time, &name, &tags, &MetricValue::Unsigned(9));
        let line = std::str::from_utf8(&export.buf).unwrap();
        assert!(line.starts_with("pref.foo.bar;"));
        assert!(line.contains("env=prod"));
        assert!(line.contains("k=v"));
        assert!(line.contains(" 9 "));
        assert!(line.ends_with('\n'));
        assert!(line.contains(&time.timestamp().to_string()));
    }

    #[test]
    fn emit_counter_uses_sum_or_diff() {
        let name = MetricName::parse("c").unwrap();
        let tags = Arc::new(MetricTagMap::default());
        let mut values = AHashMap::new();
        values.insert(
            tags,
            CounterStoreValue {
                time: Utc::now(),
                sum: MetricValue::Unsigned(100),
                diff: MetricValue::Unsigned(7),
            },
        );

        let (mut sum_export, mut sum_rx) = export(
            r#"
name: g1
server: 127.0.0.1
counter_value: sum
"#,
        );
        sum_export.emit_counter(&name, &values);
        let buf = sum_rx.try_recv().unwrap();
        assert!(std::str::from_utf8(&buf).unwrap().contains(" 100 "));

        let (mut diff_export, mut diff_rx) = export(
            r#"
name: g1
server: 127.0.0.1
counter_value: diff
"#,
        );
        diff_export.emit_counter(&name, &values);
        let buf = diff_rx.try_recv().unwrap();
        assert!(std::str::from_utf8(&buf).unwrap().contains(" 7 "));
    }

    #[test]
    fn stream_export_concatenates_pieces() {
        let export = GraphitePlaintextStreamExport::default();
        let mut buf = Vec::new();
        let n = export.serialize(&[b"a\n".to_vec(), b"b\n".to_vec()], &mut buf);
        assert_eq!(n, 2);
        assert_eq!(buf, b"a\nb\n");
    }
}
