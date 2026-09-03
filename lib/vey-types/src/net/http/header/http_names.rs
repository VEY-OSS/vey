/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::ops::Deref;

use super::HttpKnownHeader;

macro_rules! http_name {
    ($name:ident = $value:literal) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        #[allow(non_camel_case_types)]
        pub struct $name;

        impl $name {
            pub const BYTES: [u8; $value.len()] = {
                let src = $value.as_bytes();
                let mut dst = [0u8; $value.len()];
                let mut i = 0;
                while i < $value.len() {
                    dst[i] = src[i];
                    i += 1;
                }
                dst
            };
        }

        impl HttpKnownHeader for $name {
            type Bytes = [u8; $value.len()];
            const BYTES: Self::Bytes = $name::BYTES;

            fn copy(name: impl AsRef<[u8]>) -> Self::Bytes {
                name.as_ref().try_into().unwrap_or(Self::BYTES)
            }

            fn default_bytes() -> &'static [u8] {
                &$name::BYTES
            }
        }

        impl AsRef<[u8]> for $name {
            #[inline]
            fn as_ref(&self) -> &[u8] {
                &Self::BYTES
            }
        }

        impl Deref for $name {
            type Target = [u8];

            #[inline]
            fn deref(&self) -> &[u8] {
                &Self::BYTES
            }
        }
    };
}

http_name!(CONNECTION = "Connection");
http_name!(KEEP_ALIVE = "Keep-Alive");
http_name!(TRANSFER_ENCODING = "Transfer-Encoding");
http_name!(TE = "TE");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_same_length_or_default() {
        assert_eq!(&KEEP_ALIVE::copy("keep-alive"), b"keep-alive");
        assert_eq!(CONNECTION::copy("Connection"), CONNECTION::BYTES);
        assert_eq!(&TE::copy("te"), b"te");
        assert_eq!(TRANSFER_ENCODING::copy("TE"), TRANSFER_ENCODING::BYTES);
    }
}
