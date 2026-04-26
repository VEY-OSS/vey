/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use std::{fmt, str};

use arcstr::ArcStr;
use thiserror::Error;

use crate::net::{DomainName, DomainNameParseError, Host};

#[derive(Debug, Error)]
pub enum TlsServerNameError {
    #[error("not enough data: {0}")]
    NotEnoughData(usize),
    #[error("invalid list length {0}")]
    InvalidListLength(u16),
    #[error("invalid name type {0}")]
    InvalidNameType(u8),
    #[error("invalid name length {0}")]
    InvalidNameLength(usize),
    #[error("invalid host name: {0}")]
    InvalidDomain(#[from] DomainNameParseError),
}

#[derive(Clone)]
pub struct TlsServerName {
    domain: DomainName,
}

impl TlsServerName {
    pub fn from_extension_value(buf: &[u8]) -> Result<TlsServerName, TlsServerNameError> {
        let buf_len = buf.len();
        if buf_len < 5 {
            return Err(TlsServerNameError::NotEnoughData(buf_len));
        }

        let list_len = u16::from_be_bytes([buf[0], buf[1]]);
        if list_len as usize + 2 != buf_len {
            return Err(TlsServerNameError::InvalidListLength(list_len));
        }

        let name_type = buf[2];
        if name_type != 0x00 {
            return Err(TlsServerNameError::InvalidNameType(name_type));
        }

        let name_len = u16::from_be_bytes([buf[3], buf[4]]) as usize;
        if name_len + 5 > buf_len {
            return Err(TlsServerNameError::InvalidNameLength(name_len));
        }

        let name = &buf[5..5 + name_len];
        let domain = DomainName::parse(name)?;

        Ok(TlsServerName { domain })
    }
}

impl AsRef<str> for TlsServerName {
    fn as_ref(&self) -> &str {
        self.domain.as_str()
    }
}

impl From<TlsServerName> for Host {
    fn from(value: TlsServerName) -> Self {
        Host::Domain(value.domain)
    }
}

impl From<&TlsServerName> for ArcStr {
    fn from(value: &TlsServerName) -> Self {
        value.domain.as_str().into()
    }
}

impl From<&TlsServerName> for Host {
    fn from(value: &TlsServerName) -> Self {
        Host::Domain(value.domain.clone())
    }
}

impl fmt::Display for TlsServerName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.domain.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid() {
        let data: &[u8] = &[
            0x00, 0x0e, // Server Name List Length, 14
            0x00, // Server Name Type - Domain
            0x00, 0x0b, // Server Name Length, 11
            b'e', b'x', b'a', b'm', b'p', b'l', b'e', b'.', b'n', b'e', b't',
        ];
        let sni = TlsServerName::from_extension_value(data).unwrap();
        assert_eq!(sni.as_ref(), "example.net");
    }

    #[test]
    fn invalid_list_len() {
        let data: &[u8] = &[
            0x01, 0x0e, // Server Name List Length, 256 + 14
            0x00, // Server Name Type - Domain
            0x00, 0x0b, // Server Name Length, 11
            b'e', b'x', b'a', b'm', b'p', b'l', b'e', b'.', b'n', b'e', b't',
        ];
        assert!(TlsServerName::from_extension_value(data).is_err());
    }

    #[test]
    fn invalid_name_len() {
        let data: &[u8] = &[
            0x00, 0x0e, // Server Name List Length, 14
            0x00, // Server Name Type - Domain
            0x01, 0x0b, // Server Name Length, 256 + 11
            b'e', b'x', b'a', b'm', b'p', b'l', b'e', b'.', b'n', b'e', b't',
        ];
        assert!(TlsServerName::from_extension_value(data).is_err());
    }
}
