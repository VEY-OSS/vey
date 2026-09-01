/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

pub const fn connection_as_bytes(close: bool) -> &'static [u8] {
    if close {
        b"Connection: Close\r\n"
    } else {
        b"Connection: Keep-Alive\r\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_connection_as_bytes() {
        assert_eq!(connection_as_bytes(true), b"Connection: Close\r\n");
        assert_eq!(connection_as_bytes(false), b"Connection: Keep-Alive\r\n");
    }
}
