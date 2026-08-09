/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

//! High-level helpers around Intel `qatlib` asynchronous asymmetric crypto.
//!
//! Supported only on Linux `x86_64` (requires system `qatlib` via pkg-config).

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

mod ecdsa;
mod ffi;
mod mem;
mod openssl_ffi;
mod padding;
mod rsa;
mod runtime;

pub use ecdsa::ecdsa_sign_der;
pub use rsa::{rsa_decrypt, rsa_sign_pkcs1, rsa_sign_pss};
pub use runtime::QatRuntime;

use crate::ffi::CpaStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Prepare,
    Cpa(CpaStatus),
    Canceled,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Prepare => write!(f, "qat prepare failed"),
            Error::Cpa(s) => write!(f, "qat cpa status {s}"),
            Error::Canceled => write!(f, "qat operation canceled"),
        }
    }
}

impl std::error::Error for Error {}
