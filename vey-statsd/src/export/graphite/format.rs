/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use ahash::AHashMap;
use itoa::Buffer;
use jiff::Timestamp;
use tokio::sync::mpsc;

use vey_types::metrics::MetricTagMap;

use crate::config::exporter::graphite::GraphiteExporterConfig;
use crate::runtime::export::{AggregateExport, CounterStoreValue, GaugeStoreValue, StreamExport};
use crate::types::{MetricName, MetricValue};

pub(super) struct GraphitePlaintextAggregateExport {
    emit_interval: Duration,
    prefix: Option<MetricName>,
    global_tags: MetricTagMap,
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
            data_sender,
            buf: Vec::with_capacity(2048),
        }
    }

    fn serialize(
        &mut self,
        time: &Timestamp,
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
        let ts = ts_buffer.format(time.as_second());
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
        let now = Timestamp::now();
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
        let now = Timestamp::now();
        for (tags, v) in values {
            self.serialize(&now, name, tags, &v.sum);
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
        let time = "2020-01-02T03:04:05Z".parse::<Timestamp>().unwrap();
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
        assert!(line.contains(&time.as_second().to_string()));
    }

    #[test]
    fn emit_counter_writes_sum() {
        let name = MetricName::parse("c").unwrap();
        let tags = Arc::new(MetricTagMap::default());
        let mut values = AHashMap::new();
        values.insert(
            tags,
            CounterStoreValue {
                time: Timestamp::now(),
                sum: MetricValue::Unsigned(100),
                diff: MetricValue::Unsigned(7),
            },
        );

        let (mut export, mut rx) = export(
            r#"
name: g1
server: 127.0.0.1
emit_interval: 10s
"#,
        );
        export.emit_counter(&name, &values);
        let buf = rx.try_recv().unwrap();
        let text = std::str::from_utf8(&buf).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("c 100 "));
        assert!(!text.contains(".rate"));
        assert!(!text.contains(".count"));
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
