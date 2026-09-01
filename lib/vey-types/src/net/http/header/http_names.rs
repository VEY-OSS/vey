/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

pub const CONNECTION_NAME: [u8; 10] = *b"Connection";
pub const KEEP_ALIVE_NAME: [u8; 10] = *b"Keep-Alive";
pub const TRANSFER_ENCODING_NAME: [u8; 17] = *b"Transfer-Encoding";
pub const TE_NAME: [u8; 2] = *b"TE";

#[inline]
pub fn copy<const N: usize>(name: &[u8], default: [u8; N]) -> [u8; N] {
    name.try_into().unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_infers_n_from_default() {
        assert_eq!(copy(b"Keep-Alive", CONNECTION_NAME), *b"Keep-Alive");
        assert_eq!(copy(b"Connection", CONNECTION_NAME), CONNECTION_NAME);
        assert_eq!(copy(b"TE", TRANSFER_ENCODING_NAME), TRANSFER_ENCODING_NAME);
        assert_eq!(copy(b"te", TE_NAME), *b"te");
    }
}
