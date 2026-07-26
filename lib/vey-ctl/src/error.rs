/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::Utf8Error;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("cli error ({0:?})")]
    Cli(#[from] anyhow::Error),
    #[error("rpc error ({0:?})")]
    Rpc(#[from] capnp::Error),
    #[error("api error (code: {code:?}, reason: {reason:?})")]
    Api { code: i32, reason: String },
    #[error("utf8 decoding error for field {field:?}: {reason:?}")]
    Utf8 {
        field: &'static str,
        reason: Utf8Error,
    },
}

impl CommandError {
    pub fn api_error(code: i32, reason_reader: capnp::text::Reader<'_>) -> Self {
        match reason_reader.to_str() {
            Ok(reason) => CommandError::Api {
                code,
                reason: reason.to_owned(),
            },
            Err(e) => CommandError::Utf8 {
                field: "reason",
                reason: e,
            },
        }
    }
}

pub type CommandResult<T> = Result<T, CommandError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_error_ok_reason() {
        let reader = capnp::text::Reader(b"upstream unavailable");
        let err = CommandError::api_error(503, reader);
        match err {
            CommandError::Api { code, reason } => {
                assert_eq!(code, 503);
                assert_eq!(reason, "upstream unavailable");
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn api_error_invalid_utf8_reason() {
        let reader = capnp::text::Reader(&[0xff, 0xfe]);
        let err = CommandError::api_error(1, reader);
        match err {
            CommandError::Utf8 { field, .. } => assert_eq!(field, "reason"),
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn display_includes_context() {
        let err = CommandError::Api {
            code: 42,
            reason: "bad request".to_string(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("42"));
        assert!(msg.contains("bad request"));
    }
}
