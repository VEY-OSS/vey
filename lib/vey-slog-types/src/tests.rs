/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fmt::Arguments;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;

use jiff::Timestamp;
use slog::{Key, Serializer, Value};
use uuid::Uuid;
use vey_types::net::{Host, UpstreamAddr};

use crate::{LtDateTime, LtDuration, LtHost, LtIpAddr, LtSocketAddr, LtUpstreamAddr, LtUuid};

struct Capture {
    key: String,
    value: String,
}

struct CaptureSerializer(Capture);

impl CaptureSerializer {
    fn new() -> Self {
        CaptureSerializer(Capture {
            key: String::new(),
            value: String::new(),
        })
    }

    fn into_pair(self) -> (String, String) {
        (self.0.key, self.0.value)
    }
}

impl Serializer for CaptureSerializer {
    fn emit_arguments(&mut self, key: Key, val: &Arguments) -> slog::Result {
        self.0.key = key.as_str().to_string();
        self.0.value = format!("{val}");
        Ok(())
    }

    fn emit_str(&mut self, key: Key, val: &str) -> slog::Result {
        self.0.key = key.as_str().to_string();
        self.0.value = val.to_string();
        Ok(())
    }

    fn emit_none(&mut self, key: Key) -> slog::Result {
        self.0.key = key.as_str().to_string();
        self.0.value = "<none>".to_string();
        Ok(())
    }
}

fn serialize_value<V: Value>(value: V) -> (String, String) {
    static LOC: slog::RecordLocation = slog::RecordLocation {
        file: file!(),
        line: line!(),
        column: 0,
        module: module_path!(),
        function: "",
    };
    static RS: slog::RecordStatic = slog::RecordStatic {
        location: &LOC,
        tag: "",
        level: slog::Level::Info,
    };
    let msg = format_args!("");
    let record = slog::Record::new(&RS, &msg, slog::b!());
    let mut ser = CaptureSerializer::new();
    value.serialize(&record, "k".into(), &mut ser).unwrap();
    ser.into_pair()
}

#[test]
fn datetime_rfc3339_micros() {
    let dt: Timestamp = "2024-06-15T12:30:45Z".parse().unwrap();
    let (_, value) = serialize_value(LtDateTime(&dt));
    assert_eq!(value, "2024-06-15T12:30:45.000000Z");
}

#[test]
fn duration_zero_emits_none() {
    let (_, value) = serialize_value(LtDuration(Duration::ZERO));
    assert_eq!(value, "<none>");
}

#[test]
fn duration_nonzero_debug() {
    let (_, value) = serialize_value(LtDuration(Duration::from_millis(1500)));
    assert_eq!(value, "1.500s");
}

#[test]
fn ip_addr_display() {
    let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
    let (_, value) = serialize_value(LtIpAddr(ip));
    assert_eq!(value, "10.0.0.1");
}

#[test]
fn socket_addr_display() {
    let addr = SocketAddr::from_str("127.0.0.1:8080").unwrap();
    let (_, value) = serialize_value(LtSocketAddr(addr));
    assert_eq!(value, "127.0.0.1:8080");
}

#[test]
fn upstream_addr_empty_emits_none() {
    let upstream = UpstreamAddr::empty();
    let (_, value) = serialize_value(LtUpstreamAddr(&upstream));
    assert_eq!(value, "<none>");
}

#[test]
fn host_domain() {
    let host = Host::from_domain_str("example.com").unwrap();
    let (_, value) = serialize_value(LtHost(&host));
    assert_eq!(value, "example.com");
}

#[test]
fn host_ip() {
    let host = Host::Ip(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let (_, value) = serialize_value(LtHost(&host));
    assert_eq!(value, "127.0.0.1");
}

#[test]
fn host_empty_emits_none() {
    let host = Host::empty();
    let (_, value) = serialize_value(LtHost(&host));
    assert_eq!(value, "<none>");
}

#[test]
fn uuid_simple_format() {
    let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let (_, value) = serialize_value(LtUuid(&id));
    assert_eq!(value, "550e8400e29b41d4a716446655440000");
}
