/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use thiserror::Error;

use super::{FtpCommandError, FtpRawResponseError};
use crate::control::FtpCommand;

#[derive(Debug, Error)]
pub enum FtpTransferSetupError {
    #[error("raw command error: {0}")]
    RawCommandError(FtpCommandError),
    #[error("service not available")]
    ServiceNotAvailable,
    #[error("active data transfer is needed")]
    NeedActiveDataTransfer,
    #[error("data transfer not connected")]
    DataTransferNotConnected,
    #[error("data transfer connect timeout")]
    DataTransferConnectTimeout,
}

impl FtpTransferSetupError {
    /// Stop EPSV/PASV/SPSV fallback when another command on this control connection
    /// would be wrong. A rejected mode, or a data connection that never came up,
    /// still falls through to the next mode.
    pub(crate) fn skip_retry(&self) -> bool {
        match self {
            FtpTransferSetupError::DataTransferNotConnected
            | FtpTransferSetupError::DataTransferConnectTimeout => false,
            FtpTransferSetupError::RawCommandError(err) => !err.confined_to_passive_mode(),
            FtpTransferSetupError::ServiceNotAvailable => true,
            // Produced only after EPSV/PASV/SPSV fallback has already returned.
            FtpTransferSetupError::NeedActiveDataTransfer => false,
        }
    }
}

impl From<FtpCommandError> for FtpTransferSetupError {
    fn from(e: FtpCommandError) -> Self {
        match e {
            FtpCommandError::ServiceNotAvailable => FtpTransferSetupError::ServiceNotAvailable,
            _ => FtpTransferSetupError::RawCommandError(e),
        }
    }
}

#[derive(Debug, Error)]
pub enum FtpTransferServerError {
    #[error("recv failed: {0}")]
    RecvFailed(#[from] FtpRawResponseError),
    #[error("data transfer not established")]
    DataTransferNotEstablished,
    #[error("data transfer lost")]
    DataTransferLost,
    #[error("server failed")]
    ServerFailed,
    #[error("restart needed")]
    RestartNeeded,
    #[error("page type unknown")]
    PageTypeUnknown,
    #[error("exceeded storage allocation")]
    ExceededStorageAllocation,
    #[error("unexpected end reply code ({0} -> {1})")]
    UnexpectedEndReplyCode(FtpCommand, u16),
}

#[derive(Debug, Error)]
pub enum FtpLineDataReadError {
    #[error("io failed: {0:?}")]
    IoFailed(#[from] io::Error),
    #[error("line {0} is too long")]
    LineTooLong(usize),
    #[error("unsupported encoding")]
    UnsupportedEncoding,
    #[error("too many lines")]
    TooManyLines,
    #[error("aborted by callback")]
    AbortedByCallback,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(err: FtpCommandError) -> FtpTransferSetupError {
        FtpTransferSetupError::RawCommandError(err)
    }

    #[test]
    fn skip_retry_stops_when_the_control_session_cannot_continue() {
        let stop = [
            FtpTransferSetupError::ServiceNotAvailable,
            raw(FtpCommandError::SendFailed(io::Error::other("write"))),
            raw(FtpCommandError::RecvFailed(
                FtpRawResponseError::ConnectionClosed,
            )),
            raw(FtpCommandError::RecvFailed(
                FtpRawResponseError::ReadFailed(io::Error::other("read")),
            )),
            raw(FtpCommandError::RecvFailed(
                FtpRawResponseError::ReadResponseTimedOut("request epsv port"),
            )),
            raw(FtpCommandError::RecvFailed(
                FtpRawResponseError::LineTooLong,
            )),
            raw(FtpCommandError::RecvFailed(
                FtpRawResponseError::TooManyLines,
            )),
            raw(FtpCommandError::RecvFailed(
                FtpRawResponseError::InvalidLineFormat,
            )),
            raw(FtpCommandError::RecvFailed(
                FtpRawResponseError::InvalidReplyCode(0),
            )),
            raw(FtpCommandError::RecvFailed(
                FtpRawResponseError::LineIsNotUtf8,
            )),
            raw(FtpCommandError::NotLoggedIn),
            raw(FtpCommandError::BadCommandSequence(FtpCommand::EPSV)),
            raw(FtpCommandError::InvalidParameter(FtpCommand::DELE)),
            raw(FtpCommandError::PreTransferFailed(FtpCommand::LIST, 550)),
        ];

        for err in stop {
            assert!(err.skip_retry(), "{err}");
        }
    }

    #[test]
    fn skip_retry_continues_after_a_rejected_passive_mode() {
        let continue_with = [
            raw(FtpCommandError::RejectedCommandSyntax(FtpCommand::EPSV)),
            raw(FtpCommandError::CommandNotImplemented(FtpCommand::EPSV)),
            raw(FtpCommandError::ParameterNotImplemented(FtpCommand::SPDT)),
            raw(FtpCommandError::UnexpectedReplyCode(FtpCommand::EPSV, 425)),
            raw(FtpCommandError::InvalidReplySyntax(FtpCommand::EPSV, 229)),
            FtpTransferSetupError::DataTransferNotConnected,
            FtpTransferSetupError::DataTransferConnectTimeout,
        ];

        for err in continue_with {
            assert!(!err.skip_retry(), "{err}");
        }
    }
}
