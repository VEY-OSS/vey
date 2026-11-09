/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod error;
pub use error::IcapLineParseError;

mod header_line;
pub(crate) use header_line::HeaderLine;

mod status_line;
pub(crate) use status_line::StatusLine;

pub(crate) mod encapsulated;
