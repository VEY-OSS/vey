/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use thiserror::Error;

use crate::HttpLineParseError;

#[derive(Debug, Error)]
pub enum HttpUpgradeResponseError {
    #[error("too large header, should be less than {0}")]
    TooLargeHeader(usize),
    #[error("invalid status line: {0}")]
    InvalidStatusLine(HttpLineParseError),
    #[error("invalid header line: {0}")]
    InvalidHeaderLine(HttpLineParseError),
    #[error("unsupported value in header {0}")]
    UnsupportedHeaderValue(&'static str),
    #[error("upgrade token not match")]
    UpgradeTokenNotMatch,
    #[error("invalid chunked transfer-encoding")]
    InvalidChunkedTransferEncoding,
    #[error("invalid content length")]
    InvalidContentLength,
}

#[derive(Debug, Error)]
pub enum HttpUpgradeError {
    #[error("remote closed")]
    RemoteClosed,
    #[error("read failed: {0:?}")]
    ReadFailed(io::Error),
    #[error("write failed: {0:?}")]
    WriteFailed(io::Error),
    #[error("invalid response: {0}")]
    InvalidResponse(#[from] HttpUpgradeResponseError),
    #[error("unexpected status code {0} {1}")]
    UnexpectedStatusCode(u16, String),
    #[error("peer timeout with status code {0}")]
    PeerTimeout(u16),
}
