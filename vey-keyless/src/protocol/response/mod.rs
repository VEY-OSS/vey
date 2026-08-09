/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use thiserror::Error;

pub(crate) mod cloudflare;

pub(crate) struct KeylessPongResponse {
    pub(crate) id: u32,
    pub(crate) payload: Vec<u8>,
}

impl KeylessPongResponse {
    pub(crate) fn new(id: u32, payload: &[u8]) -> Self {
        KeylessPongResponse {
            id,
            payload: payload.to_vec(),
        }
    }
}

pub(crate) struct KeylessDataResponse {
    pub(crate) id: u32,
    pub(crate) payload: Vec<u8>,
}

impl KeylessDataResponse {
    pub(crate) fn new(id: u32, key_size: usize) -> Self {
        KeylessDataResponse {
            id,
            payload: vec![0u8; key_size],
        }
    }

    #[cfg(any(
        all(feature = "crypto-mb", target_arch = "x86_64"),
        all(feature = "qat", target_os = "linux", target_arch = "x86_64"),
    ))]
    pub(crate) fn with_payload(id: u32, payload: impl Into<Vec<u8>>) -> Self {
        KeylessDataResponse {
            id,
            payload: payload.into(),
        }
    }

    pub(crate) fn payload_data_mut(&mut self) -> &mut [u8] {
        &mut self.payload
    }

    pub(crate) fn finalize_payload(&mut self, payload_len: usize) {
        self.payload.truncate(payload_len);
    }
}

#[allow(unused)]
#[derive(Clone, Copy, Debug, Error)]
#[repr(u8)]
pub(crate) enum KeylessResponseErrorCode {
    #[error("no error")]
    NoError = 0,
    #[error("cryptography failure")]
    CryptographyFailure = 1,
    #[error("no matching certificate ID")]
    KeyNotFound = 2,
    #[error("I/O read failure")]
    ReadError = 3,
    #[error("unsupported version incorrect")]
    VersionMismatch = 4,
    #[error("use of unknown opcode in request")]
    BadOpCode = 5,
    #[error("use of unexpected opcode in request")]
    UnexpectedOpCode = 6,
    #[error("malformed message")]
    FormatError = 7,
    #[error("memory or other internal error")]
    InternalError = 8,
    #[error("certificate not found")]
    CertNotFound = 9,
    #[error("sealing key expired")]
    Expired = 10,
    #[error("the remote keyserver was not configured correctly")]
    RemoteConfiguration = 11,
}

#[derive(Clone, Copy)]
pub(crate) struct KeylessErrorResponse {
    pub(crate) id: u32,
    pub(crate) code: KeylessResponseErrorCode,
}

impl KeylessErrorResponse {
    pub(crate) fn new(id: u32) -> Self {
        KeylessErrorResponse {
            id,
            code: KeylessResponseErrorCode::NoError,
        }
    }

    pub(crate) fn error_code(&self) -> KeylessResponseErrorCode {
        self.code
    }

    fn set_error_code(mut self, error_code: KeylessResponseErrorCode) -> Self {
        self.code = error_code;
        self
    }

    #[inline]
    pub(crate) fn key_not_found(self) -> Self {
        self.set_error_code(KeylessResponseErrorCode::KeyNotFound)
    }

    #[inline]
    pub(crate) fn bad_op_code(self) -> Self {
        self.set_error_code(KeylessResponseErrorCode::BadOpCode)
    }

    #[inline]
    pub(crate) fn unexpected_op_code(self) -> Self {
        self.set_error_code(KeylessResponseErrorCode::UnexpectedOpCode)
    }

    #[inline]
    pub(crate) fn crypto_fail(self) -> Self {
        self.set_error_code(KeylessResponseErrorCode::CryptographyFailure)
    }

    #[inline]
    pub(crate) fn format_error(self) -> Self {
        self.set_error_code(KeylessResponseErrorCode::FormatError)
    }
}

pub(crate) enum KeylessResponse {
    Data(KeylessDataResponse),
    Pong(KeylessPongResponse),
    Error(KeylessErrorResponse),
}

impl KeylessResponse {
    #[allow(unused)]
    pub(crate) fn id(&self) -> u32 {
        match self {
            KeylessResponse::Data(d) => d.id,
            KeylessResponse::Pong(p) => p.id,
            KeylessResponse::Error(e) => e.id,
        }
    }
}
