/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::cell::RefCell;
use std::fmt::{Arguments, Write};
use std::io;

use jiff::Timestamp;
use serde::ser::Serialize;
use slog::{KV, OwnedKVList, Record, Serializer};

use vey_types::log::AsyncLogFormatter;

thread_local! {
    static TL_BUF: RefCell<String> = RefCell::new(String::with_capacity(128))
}

struct EncodeError(slog::Error);

impl From<rmp::encode::ValueWriteError> for EncodeError {
    fn from(e: rmp::encode::ValueWriteError) -> Self {
        EncodeError(slog::Error::Io(e.into()))
    }
}

impl From<slog::Error> for EncodeError {
    fn from(e: slog::Error) -> Self {
        EncodeError(e)
    }
}

pub struct FluentdFormatter {
    tag_name: String,
}

impl FluentdFormatter {
    pub(super) fn new(tag_name: String) -> Self {
        FluentdFormatter { tag_name }
    }

    fn rmp_encode(
        &self,
        record: &Record,
        logger_values: &OwnedKVList,
    ) -> Result<Vec<u8>, EncodeError> {
        let datetime_now = Timestamp::now();
        let mut buf = Vec::<u8>::with_capacity(1024);

        rmp::encode::write_array_len(&mut buf, 3)?;
        {
            // #1
            rmp::encode::write_str(&mut buf, &self.tag_name)?;

            // #2
            rmp::encode::write_ext_meta(&mut buf, 8, 0)?;
            let sec = u32::try_from(datetime_now.as_second())
                .map_err(|_| slog::Error::Io(io::Error::other("out of range unix timestamp")))?
                .to_be_bytes();
            buf.extend_from_slice(&sec);
            // jiff has no leap seconds; subsec is already in 0..=999_999_999 for now().
            let nano = (datetime_now.subsec_nanosecond() as u32).to_be_bytes();
            buf.extend_from_slice(&nano);

            // #3
            let mut counter = CounterKV(0);
            logger_values.serialize(record, &mut counter)?;
            record.kv().serialize(record, &mut counter)?;
            rmp::encode::write_map_len(&mut buf, counter.0 + 1)?;
            {
                let mut kv_formatter = FormatterKv(&mut buf);
                logger_values.serialize(record, &mut kv_formatter)?;
                record.kv().serialize(record, &mut kv_formatter)?;
                kv_formatter.emit_arguments("msg".into(), record.msg())?;
            }
        }

        Ok(buf)
    }
}

impl AsyncLogFormatter<Vec<u8>> for FluentdFormatter {
    fn format_slog(
        &self,
        record: &Record,
        logger_values: &OwnedKVList,
    ) -> Result<Vec<u8>, slog::Error> {
        let buf = self.rmp_encode(record, logger_values).map_err(|e| e.0)?;
        Ok(buf)
    }
}

struct CounterKV(u32);

impl Serializer for CounterKV {
    fn emit_arguments(&mut self, _key: slog::Key, _val: &Arguments) -> slog::Result {
        self.0 += 1;
        Ok(())
    }
}

struct FormatterKv<'a>(&'a mut Vec<u8>);

impl FormatterKv<'_> {
    fn write_key(&mut self, key: &str) -> slog::Result {
        rmp::encode::write_str(&mut self.0, key).map_err(|e| slog::Error::Io(e.into()))
    }
}

impl Serializer for FormatterKv<'_> {
    fn emit_usize(&mut self, key: slog::Key, value: usize) -> slog::Result {
        self.emit_u64(key, value as u64)
    }
    fn emit_isize(&mut self, key: slog::Key, value: isize) -> slog::Result {
        self.emit_i64(key, value as i64)
    }

    impl_encode! {
        u8 => emit_u8, write_u8
    }
    impl_encode! {
        i8 => emit_i8, write_i8
    }
    impl_encode! {
        u16 => emit_u16, write_u16
    }
    impl_encode! {
        i16 => emit_i16, write_i16
    }
    impl_encode! {
        u32 => emit_u32, write_u32
    }
    impl_encode! {
        i32 => emit_i32, write_i32
    }
    impl_encode! {
        u64 => emit_u64, write_u64
    }
    impl_encode! {
        i64 => emit_i64, write_i64
    }

    impl_encode! {
        f32 => emit_f32, write_f32
    }
    impl_encode! {
        f64 => emit_f64, write_f64
    }

    impl_encode! {
        bool => emit_bool, write_bool
    }

    fn emit_char(&mut self, key: slog::Key, value: char) -> slog::Result {
        self.emit_str(key, value.encode_utf8(&mut [0u8; char::MAX_LEN_UTF8]))
    }

    fn emit_none(&mut self, key: slog::Key) -> slog::Result {
        self.write_key(key.as_str())?;
        rmp::encode::write_nil(&mut self.0).map_err(slog::Error::Io)
    }

    fn emit_str(&mut self, key: slog::Key, value: &str) -> slog::Result {
        self.write_key(key.as_str())?;
        rmp::encode::write_str(&mut self.0, value).map_err(|e| slog::Error::Io(e.into()))
    }

    fn emit_arguments(&mut self, key: slog::Key, value: &Arguments) -> slog::Result {
        if let Some(s) = value.as_str() {
            self.emit_str(key, s)
        } else {
            TL_BUF.with_borrow_mut(|buf| {
                buf.clear();

                buf.write_fmt(*value).map_err(slog::Error::Fmt)?;

                self.emit_str(key, buf.as_str())
            })
        }
    }

    fn emit_serde(&mut self, key: slog::Key, value: &dyn slog::SerdeValue) -> slog::Result {
        self.write_key(key.as_str())?;
        let mut serializer = rmp_serde::Serializer::new(&mut self.0);
        value.as_serde().serialize(&mut serializer).map_err(|e| {
            io::Error::other(format!("serde serialization error for key {key}: {e}"))
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slog::{OwnedKVList, Record, RecordLocation, RecordStatic, Serializer, b, o};

    #[test]
    fn emit_u64_writes_key_and_value() {
        let mut buf = Vec::new();
        let mut fmt = FormatterKv(&mut buf);
        fmt.emit_u64("count".into(), 42).unwrap();
        assert!(buf.array_windows::<5>().any(|w| w == b"count"));
    }

    #[test]
    fn emit_none_writes_nil() {
        let mut buf = Vec::new();
        let mut fmt = FormatterKv(&mut buf);
        fmt.emit_none("empty".into()).unwrap();
        assert!(buf.array_windows::<5>().any(|w| w == b"empty"));
        assert!(buf.contains(&0xc0));
    }

    #[test]
    fn emit_str_writes_key_and_value() {
        let mut buf = Vec::new();
        let mut fmt = FormatterKv(&mut buf);
        fmt.emit_str("msg".into(), "hello").unwrap();
        assert!(buf.array_windows::<3>().any(|w| w == b"msg"));
        assert!(buf.array_windows::<5>().any(|w| w == b"hello"));
    }

    #[test]
    fn emit_scalar_types() {
        let mut buf = Vec::new();
        let mut fmt = FormatterKv(&mut buf);
        fmt.emit_bool("ok".into(), true).unwrap();
        fmt.emit_i64("neg".into(), -7).unwrap();
        fmt.emit_f64("pi".into(), 3.5).unwrap();
        fmt.emit_char("ch".into(), 'Z').unwrap();
        assert!(buf.array_windows::<2>().any(|w| w == b"ok"));
        assert!(buf.array_windows::<3>().any(|w| w == b"neg"));
        assert!(buf.array_windows::<2>().any(|w| w == b"pi"));
        assert!(buf.array_windows::<2>().any(|w| w == b"ch"));
        assert!(buf.array_windows::<1>().any(|w| w == b"Z"));
        assert!(buf.contains(&0xc3)); // true
    }

    #[test]
    fn emit_arguments_static_and_formatted() {
        let mut buf = Vec::new();
        let mut fmt = FormatterKv(&mut buf);
        fmt.emit_arguments("static".into(), &format_args!("plain"))
            .unwrap();
        fmt.emit_arguments("fmt".into(), &format_args!("n={}", 9))
            .unwrap();
        assert!(buf.array_windows::<5>().any(|w| w == b"plain"));
        assert!(buf.array_windows::<3>().any(|w| w == b"n=9"));
    }

    #[test]
    fn counter_kv_counts_emits() {
        let mut counter = CounterKV(0);
        counter
            .emit_arguments("a".into(), &format_args!("1"))
            .unwrap();
        counter
            .emit_arguments("b".into(), &format_args!("2"))
            .unwrap();
        assert_eq!(counter.0, 2);
    }

    #[test]
    fn format_slog_encodes_fluent_forward_shape() {
        static LOC: RecordLocation = RecordLocation {
            file: file!(),
            line: line!(),
            column: 0,
            module: module_path!(),
            function: "",
        };
        static RS: RecordStatic = RecordStatic {
            location: &LOC,
            tag: "",
            level: slog::Level::Info,
        };

        let fmt = FluentdFormatter::new("vey.app".to_owned());
        let msg = format_args!("hello {}", "world");
        let kv = b!("count" => 3u64);
        let record = Record::new(&RS, &msg, kv);
        let owned: OwnedKVList = o!("host" => "local").into();
        let buf = fmt.format_slog(&record, &owned).unwrap();

        let mut bytes = rmp::decode::Bytes::new(&buf);
        assert_eq!(rmp::decode::read_array_len(&mut bytes).unwrap(), 3);

        let (tag, rest) = rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
        assert_eq!(tag, "vey.app");
        bytes = rmp::decode::Bytes::new(rest);

        let ext = rmp::decode::read_ext_meta(&mut bytes).unwrap();
        assert_eq!(ext.typeid, 0);
        assert_eq!(ext.size, 8);
        let rem = bytes.remaining_slice();
        bytes = rmp::decode::Bytes::new(&rem[8..]);

        let map_len = rmp::decode::read_map_len(&mut bytes).unwrap();
        assert_eq!(map_len, 3); // host + count + msg

        let mut saw_msg = false;
        let mut saw_host = false;
        let mut saw_count = false;
        for _ in 0..map_len {
            let (key, rest) = rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
            bytes = rmp::decode::Bytes::new(rest);
            match key {
                "msg" => {
                    let (value, rest) =
                        rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
                    assert_eq!(value, "hello world");
                    bytes = rmp::decode::Bytes::new(rest);
                    saw_msg = true;
                }
                "host" => {
                    let (value, rest) =
                        rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
                    assert_eq!(value, "local");
                    bytes = rmp::decode::Bytes::new(rest);
                    saw_host = true;
                }
                "count" => {
                    let value = rmp::decode::read_int::<u64, _>(&mut bytes).unwrap();
                    assert_eq!(value, 3);
                    saw_count = true;
                }
                other => panic!("unexpected key {other}"),
            }
        }
        assert!(saw_msg && saw_host && saw_count);
    }
}
