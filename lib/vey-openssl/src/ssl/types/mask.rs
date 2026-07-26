/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use bitflags::bitflags;

bitflags! {
    pub struct SslInfoCallbackWhere: i32 {
        const LOOP = 0x01;
        const EXIT = 0x02;
        const READ = 0x04;
        const WRITE = 0x08;
        const HANDSHAKE_START = 0x10;
        const HANDSHAKE_DONE = 0x20;
        const ALERT = 0x4000;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_expected_flags() {
        let flags = SslInfoCallbackWhere::READ | SslInfoCallbackWhere::WRITE;
        assert!(flags.contains(SslInfoCallbackWhere::READ));
        assert!(flags.contains(SslInfoCallbackWhere::WRITE));
        assert!(!flags.contains(SslInfoCallbackWhere::HANDSHAKE_DONE));
    }

    #[test]
    fn handshake_flags_have_expected_bits() {
        assert_eq!(SslInfoCallbackWhere::HANDSHAKE_START.bits(), 0x10);
        assert_eq!(SslInfoCallbackWhere::HANDSHAKE_DONE.bits(), 0x20);
        assert_eq!(SslInfoCallbackWhere::ALERT.bits(), 0x4000);
    }

    #[test]
    fn all_flags_combine_without_overlap() {
        let all = SslInfoCallbackWhere::LOOP
            | SslInfoCallbackWhere::EXIT
            | SslInfoCallbackWhere::READ
            | SslInfoCallbackWhere::WRITE
            | SslInfoCallbackWhere::HANDSHAKE_START
            | SslInfoCallbackWhere::HANDSHAKE_DONE
            | SslInfoCallbackWhere::ALERT;
        assert!(all.contains(SslInfoCallbackWhere::LOOP));
        assert!(all.contains(SslInfoCallbackWhere::ALERT));
        assert_eq!(all.bits(), 0x403F);
    }
}
