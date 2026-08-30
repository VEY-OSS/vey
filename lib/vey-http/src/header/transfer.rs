/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

pub const TRANSFER_ENCODING_NAME: [u8; 17] = *b"Transfer-Encoding";

pub fn transfer_encoding_chunked() -> &'static str {
    "Transfer-Encoding: chunked\r\n"
}
