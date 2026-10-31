/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use thiserror::Error;

#[derive(Debug, Error)]
pub enum XCryptParseError {
    #[error("unknown prefix")]
    UnknownPrefix,
    #[error("invalid rounds")]
    InvalidRounds,
    #[error("out of range rounds")]
    OutOfRangeRounds,
    #[error("no salt found")]
    NoSaltFound,
    #[error("salt too long")]
    SaltTooLong,
    #[error("invalid hash size")]
    InvalidHashSize,
    #[error("invalid hash")]
    InvalidHash,
}

pub type XCryptParseResult<T> = Result<T, XCryptParseError>;
