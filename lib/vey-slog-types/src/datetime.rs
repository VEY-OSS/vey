/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use jiff::Timestamp;
use slog::{Record, Serializer, Value};

use vey_datetime::DateTimeFormatExt;

pub struct LtDateTime<'a>(pub &'a Timestamp);

impl Value for LtDateTime<'_> {
    fn serialize(
        &self,
        _record: &Record,
        key: slog::Key,
        serializer: &mut dyn Serializer,
    ) -> slog::Result {
        let s = self.0.format_rfc3339_fixed_microsecond().to_string();
        serializer.emit_str(key, &s)
    }
}
