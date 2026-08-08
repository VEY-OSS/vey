/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::cell::RefCell;
use std::fmt::{Arguments, Write};
use std::io;

use serde::ser::SerializeMap;
use slog::Serializer;

thread_local! {
    static TL_BUF: RefCell<String> = RefCell::new(String::with_capacity(128))
}

pub(super) struct SerdeFormatterKV<S: serde::Serializer> {
    ser_map: S::SerializeMap,
}

impl<S: serde::Serializer> SerdeFormatterKV<S> {
    /// Start serializing map of values
    pub(super) fn start(ser: S, len: Option<usize>) -> Result<Self, slog::Error> {
        let ser_map = ser
            .serialize_map(len)
            .map_err(|e| io::Error::other(format!("serde serialization error: {e}")))?;
        Ok(SerdeFormatterKV { ser_map })
    }

    /// Finish serialization, and return the serializer
    pub(super) fn end(self) -> Result<S::Ok, S::Error> {
        self.ser_map.end()
    }
}

macro_rules! impl_m(
    ($s:expr, $key:expr, $val:expr) => ({
        let k_s:  &str = $key.as_ref();
        $s.ser_map.serialize_entry(k_s, $val)
             .map_err(|e| io::Error::other(format!("serde serialization error: {e}")))?;
        Ok(())
    });
);

impl<S: serde::Serializer> Serializer for SerdeFormatterKV<S> {
    fn emit_bool(&mut self, key: slog::Key, value: bool) -> slog::Result {
        impl_m!(self, key, &value)
    }

    fn emit_unit(&mut self, key: slog::Key) -> slog::Result {
        impl_m!(self, key, &())
    }

    fn emit_char(&mut self, key: slog::Key, value: char) -> slog::Result {
        impl_m!(self, key, &value)
    }

    fn emit_none(&mut self, _key: slog::Key) -> slog::Result {
        Ok(())
    }
    fn emit_u8(&mut self, key: slog::Key, value: u8) -> slog::Result {
        impl_m!(self, key, &value)
    }
    fn emit_i8(&mut self, key: slog::Key, value: i8) -> slog::Result {
        impl_m!(self, key, &value)
    }
    fn emit_u16(&mut self, key: slog::Key, value: u16) -> slog::Result {
        impl_m!(self, key, &value)
    }
    fn emit_i16(&mut self, key: slog::Key, value: i16) -> slog::Result {
        impl_m!(self, key, &value)
    }
    fn emit_usize(&mut self, key: slog::Key, value: usize) -> slog::Result {
        impl_m!(self, key, &value)
    }
    fn emit_isize(&mut self, key: slog::Key, value: isize) -> slog::Result {
        impl_m!(self, key, &value)
    }
    fn emit_u32(&mut self, key: slog::Key, value: u32) -> slog::Result {
        impl_m!(self, key, &value)
    }
    fn emit_i32(&mut self, key: slog::Key, value: i32) -> slog::Result {
        impl_m!(self, key, &value)
    }
    fn emit_f32(&mut self, key: slog::Key, value: f32) -> slog::Result {
        impl_m!(self, key, &value)
    }
    fn emit_u64(&mut self, key: slog::Key, value: u64) -> slog::Result {
        impl_m!(self, key, &value)
    }
    fn emit_i64(&mut self, key: slog::Key, value: i64) -> slog::Result {
        impl_m!(self, key, &value)
    }
    fn emit_f64(&mut self, key: slog::Key, value: f64) -> slog::Result {
        impl_m!(self, key, &value)
    }
    fn emit_str(&mut self, key: slog::Key, value: &str) -> slog::Result {
        impl_m!(self, key, &value)
    }

    impl_arguments_with_tls! {}

    fn emit_serde(&mut self, key: slog::Key, value: &dyn slog::SerdeValue) -> slog::Result {
        self.ser_map
            .serialize_entry(key.as_str(), value.as_serde())
            .map_err(|e| {
                io::Error::other(format!("serde serialization error for key {key}: {e}"))
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slog::Serializer;

    #[test]
    fn serde_formatter_kv_emits_json_object() {
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::new(&mut buf);
        let mut kv = SerdeFormatterKV::start(&mut ser, Some(3)).unwrap();
        kv.emit_str("msg".into(), "hi").unwrap();
        kv.emit_u8("n".into(), 7).unwrap();
        kv.emit_bool("ok".into(), true).unwrap();
        kv.end().unwrap();

        let value: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(value["msg"], "hi");
        assert_eq!(value["n"], 7);
        assert_eq!(value["ok"], true);
    }

    #[test]
    fn serde_formatter_kv_skips_none() {
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::new(&mut buf);
        let mut kv = SerdeFormatterKV::start(&mut ser, None).unwrap();
        kv.emit_none("missing".into()).unwrap();
        kv.emit_i64("report_ts".into(), 123).unwrap();
        kv.end().unwrap();

        let value: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert!(value.get("missing").is_none());
        assert_eq!(value["report_ts"], 123);
    }
}
