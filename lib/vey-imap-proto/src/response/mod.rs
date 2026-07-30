/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::{self, Utf8Error};

use atoi::FromRadix10Checked;
use log::trace;
use smol_str::SmolStr;
use thiserror::Error;

mod bad;
pub use bad::BadResponse;

mod bye;
pub use bye::ByeResponse;

#[derive(Debug, Error)]
pub enum ResponseLineError {
    #[error("no trailing sequence")]
    NoTrailingSequence,
    #[error("no tag found as a prefix")]
    NotTagPrefixed,
    #[error("invalid utf-8 response: {0}")]
    InvalidUtf8Response(Utf8Error),
    #[error("no result field found")]
    NoResultField,
    #[error("invalid tagged result")]
    InvalidTaggedResult,
    #[error("unknown untagged result")]
    UnknownUntaggedResult,
    #[error("invalid literal size")]
    InvalidLiteralSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandResult {
    Success,
    Fail,
    ProtocolError,
}

pub struct TaggedResponse {
    pub tag: SmolStr,
    pub result: CommandResult,
}

impl TaggedResponse {
    fn parse(tag: &[u8], left: &[u8]) -> Result<Self, ResponseLineError> {
        let tag = str::from_utf8(tag).map_err(ResponseLineError::InvalidUtf8Response)?;
        let tag = SmolStr::from(tag);

        let Some(d) = memchr::memchr(b' ', left) else {
            return Err(ResponseLineError::NoResultField);
        };
        let result = str::from_utf8(&left[..d]).map_err(ResponseLineError::InvalidUtf8Response)?;
        let result = match result.to_uppercase().as_str() {
            "OK" => CommandResult::Success,
            "NO" => CommandResult::Fail,
            "BAD" => CommandResult::ProtocolError,
            _ => return Err(ResponseLineError::InvalidTaggedResult),
        };
        Ok(TaggedResponse { tag, result })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerStatus {
    Information,
    Warning,
    Error,
    Authenticated,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandData {
    Enabled,
    Capability,
    Fetch,
    Id,
    Other,
}

pub struct UntaggedResponse {
    pub command_data: CommandData,
    pub literal_data: Option<u64>,
}

impl UntaggedResponse {
    pub fn parse_continue_line(&mut self, line: &[u8]) -> Result<(), ResponseLineError> {
        let left = line
            .strip_suffix(b"\r\n")
            .ok_or(ResponseLineError::NoTrailingSequence)?;

        #[cfg(debug_assertions)]
        if let Ok(s) = str::from_utf8(left) {
            trace!("[IMAP] +-< {s}");
        }

        if left.is_empty() {
            self.literal_data = None;
        } else {
            self.literal_data = check_literal_size(left)?;
        }

        Ok(())
    }
}

pub enum Response {
    CommandResult(TaggedResponse),
    ServerStatus(ServerStatus),
    CommandData(UntaggedResponse),
    ContinuationRequest,
}

impl Response {
    pub fn parse_line(line: &[u8]) -> Result<Self, ResponseLineError> {
        let left = line
            .strip_suffix(b"\r\n")
            .ok_or(ResponseLineError::NoTrailingSequence)?;

        #[cfg(debug_assertions)]
        if let Ok(s) = str::from_utf8(left) {
            trace!("[IMAP] --< {s}");
        }

        let Some(d) = memchr::memchr(b' ', left) else {
            return Err(ResponseLineError::NotTagPrefixed);
        };

        match left[0] {
            b' ' => Err(ResponseLineError::NotTagPrefixed),
            b'*' => Self::parse_untagged(&left[d + 1..]),
            b'+' => Ok(Response::ContinuationRequest),
            _ => TaggedResponse::parse(&left[..d], &left[d + 1..]).map(Response::CommandResult),
        }
    }

    fn parse_untagged(left: &[u8]) -> Result<Self, ResponseLineError> {
        match memchr::memchr(b' ', left) {
            Some(d) => {
                let r1 =
                    str::from_utf8(&left[..d]).map_err(ResponseLineError::InvalidUtf8Response)?;
                match r1.to_uppercase().as_str() {
                    "OK" => Ok(Response::ServerStatus(ServerStatus::Information)),
                    "NO" => Ok(Response::ServerStatus(ServerStatus::Warning)),
                    "BAD" => Ok(Response::ServerStatus(ServerStatus::Error)),
                    "PREAUTH" => Ok(Response::ServerStatus(ServerStatus::Authenticated)),
                    "BYE" => Ok(Response::ServerStatus(ServerStatus::Close)),
                    "ENABLED" => {
                        // rfc5161, rev2
                        Ok(Response::CommandData(UntaggedResponse {
                            command_data: CommandData::Enabled,
                            literal_data: None,
                        }))
                    }
                    "CAPABILITY" => Ok(Response::CommandData(UntaggedResponse {
                        command_data: CommandData::Capability,
                        literal_data: None,
                    })),
                    "ID" => {
                        // rfc2971, rev2
                        Ok(Response::CommandData(UntaggedResponse {
                            command_data: CommandData::Id,
                            literal_data: None,
                        }))
                    },
                    "LIST"
                    | "LSUB" // rev1
                    | "NAMESPACE" // rfc2342, rev2
                    | "STATUS" | "SEARCH"
                    | "ESEARCH" // rfc4731, rev2
                    | "FLAGS" => {
                        Ok(Response::CommandData(UntaggedResponse {
                            command_data: CommandData::Other,
                            literal_data: None,
                        }))
                    }
                    "SORT" | "THREAD" => {
                        // rfc5256
                        Ok(Response::CommandData(UntaggedResponse {
                            command_data: CommandData::Other,
                            literal_data: None,
                        }))
                    },
                    "LANGUAGE" | "COMPARATOR" => {
                        // rfc5255
                        Ok(Response::CommandData(UntaggedResponse {
                            command_data: CommandData::Other,
                            literal_data: None,
                        }))
                    },
                    "VANISHED" => {
                        // rfc7162
                        Ok(Response::CommandData(UntaggedResponse {
                            command_data: CommandData::Other,
                            literal_data: None,
                        }))
                    }
                    "QUOTA" | "QUOTAROOT" => {
                        // rfc9208
                        Ok(Response::CommandData(UntaggedResponse {
                            command_data: CommandData::Other,
                            literal_data: None,
                        }))
                    },
                    "ACL" | "LISTRIGHTS" | "MYRIGHTS" => {
                        // rfc4314
                        Ok(Response::CommandData(UntaggedResponse {
                            command_data: CommandData::Other,
                            literal_data: None,
                        }))
                    }
                    "CONVERSION" | "CONVERTED" => {
                        // rfc5259
                        Ok(Response::CommandData(UntaggedResponse {
                            command_data: CommandData::Other,
                            literal_data: None,
                        }))
                    }
                    "METADATA" => {
                        // rfc5464
                        Ok(Response::CommandData(UntaggedResponse {
                            command_data: CommandData::Other,
                            literal_data: None,
                        }))
                    }
                    "GENURLAUTH" | "URLFETCH" => {
                        // rfc4467
                        Ok(Response::CommandData(UntaggedResponse {
                            command_data: CommandData::Other,
                            literal_data: None,
                        }))
                    }
                    _ => {
                        let left = &left[d + 1..];
                        match memchr::memchr(b' ', left) {
                            Some(d) => {
                                let r2 = str::from_utf8(&left[..d])
                                    .map_err(ResponseLineError::InvalidUtf8Response)?;
                                match r2.to_uppercase().as_str() {
                                    "FETCH" => {
                                        let literal_data = check_literal_size(left)?;
                                        Ok(Response::CommandData(UntaggedResponse {
                                            command_data: CommandData::Fetch,
                                            literal_data,
                                        }))
                                    }
                                    _ => {
                                        trace!("unknown IMAP response line: * {r1} {r2} ...");
                                        Err(ResponseLineError::UnknownUntaggedResult)
                                    }
                                }
                            }
                            None => {
                                let r2 = str::from_utf8(left)
                                    .map_err(ResponseLineError::InvalidUtf8Response)?;
                                match r2.to_uppercase().as_str() {
                                    "EXISTS" | "EXPUNGE" | "RECENT" => {
                                        Ok(Response::CommandData(UntaggedResponse {
                                            command_data: CommandData::Other,
                                            literal_data: None,
                                        }))
                                    }
                                    _ => {
                                        trace!("unknown IMAP response line: * {r1} {r2}");
                                        Err(ResponseLineError::UnknownUntaggedResult)
                                    }
                                }
                            }
                        }
                    }
                }
            }
            None => {
                let r1 = str::from_utf8(left).map_err(ResponseLineError::InvalidUtf8Response)?;
                match r1.to_uppercase().as_str() {
                    "SEARCH" => Ok(Response::CommandData(UntaggedResponse {
                        command_data: CommandData::Other,
                        literal_data: None,
                    })),
                    _ => {
                        trace!("unknown IMAP response line: * {r1}");
                        Err(ResponseLineError::UnknownUntaggedResult)
                    }
                }
            }
        }
    }
}

fn check_literal_size(left: &[u8]) -> Result<Option<u64>, ResponseLineError> {
    if left.ends_with(b"}")
        && let Some(p) = memchr::memrchr(b'{', left)
    {
        let size_s = &left[p + 1..left.len() - 1];
        if size_s.is_empty() {
            return Err(ResponseLineError::InvalidLiteralSize);
        }
        let (size, offset) = u64::from_radix_10_checked(size_s);
        let Some(size) = size else {
            return Err(ResponseLineError::InvalidLiteralSize);
        };
        if offset == size_s.len() || (offset + 1 == size_s.len() && size_s[offset] == b'+') {
            return Ok(Some(size));
        }
        return Err(ResponseLineError::InvalidLiteralSize);
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &[u8]) -> Response {
        Response::parse_line(line).unwrap()
    }

    fn command_data(line: &[u8]) -> UntaggedResponse {
        let Response::CommandData(r) = parse(line) else {
            panic!("expected command data for {line:?}")
        };
        r
    }

    fn server_status(line: &[u8]) -> ServerStatus {
        let Response::ServerStatus(status) = parse(line) else {
            panic!("expected server status for {line:?}")
        };
        status
    }

    fn tagged(line: &[u8]) -> TaggedResponse {
        let Response::CommandResult(r) = parse(line) else {
            panic!("expected tagged response for {line:?}")
        };
        r
    }

    #[test]
    fn bye() {
        assert_eq!(
            server_status(b"* BYE Autologout; idle for too long\r\n"),
            ServerStatus::Close
        );
    }

    #[test]
    fn capability() {
        let r = command_data(
            b"* CAPABILITY STARTTLS AUTH=GSSAPI IMAP4rev2 LOGINDISABLED XPIG-LATIN\r\n",
        );
        assert_eq!(r.command_data, CommandData::Capability);
        assert!(r.literal_data.is_none());
    }

    #[test]
    fn exists() {
        let r = command_data(b"* 23 EXISTS\r\n");
        assert_eq!(r.command_data, CommandData::Other);
        assert!(r.literal_data.is_none());
    }

    #[test]
    fn fetch() {
        let r = command_data(b"* 12 FETCH (BODY[HEADER] {342}\r\n");
        assert_eq!(r.command_data, CommandData::Fetch);
        assert_eq!(r.literal_data, Some(342));

        let r = command_data(b"* 12 FETCH (BODY[HEADER] {342+}\r\n");
        assert_eq!(r.command_data, CommandData::Fetch);
        assert_eq!(r.literal_data, Some(342));

        let r = command_data(b"* 12 FETCH (FLAGS (\\Seen))\r\n");
        assert_eq!(r.command_data, CommandData::Fetch);
        assert!(r.literal_data.is_none());
    }

    #[test]
    fn tagged_results() {
        let r = tagged(b"A001 OK CAPABILITY completed\r\n");
        assert_eq!(r.tag.as_str(), "A001");
        assert_eq!(r.result, CommandResult::Success);

        let r = tagged(b"A002 NO login failed\r\n");
        assert_eq!(r.result, CommandResult::Fail);

        let r = tagged(b"A003 BAD Command unknown\r\n");
        assert_eq!(r.result, CommandResult::ProtocolError);

        let r = tagged(b"a004 ok done\r\n");
        assert_eq!(r.tag.as_str(), "a004");
        assert_eq!(r.result, CommandResult::Success);
    }

    #[test]
    fn continuation_request() {
        assert!(matches!(
            parse(b"+ Ready for literal data\r\n"),
            Response::ContinuationRequest
        ));
        assert!(matches!(parse(b"+ \r\n"), Response::ContinuationRequest));
    }

    #[test]
    fn server_status_variants() {
        assert_eq!(
            server_status(b"* OK IMAP4rev1 Service Ready\r\n"),
            ServerStatus::Information
        );
        assert_eq!(
            server_status(b"* NO [ALERT] System too busy\r\n"),
            ServerStatus::Warning
        );
        assert_eq!(
            server_status(b"* BAD Command line too long\r\n"),
            ServerStatus::Error
        );
        assert_eq!(
            server_status(b"* PREAUTH welcome\r\n"),
            ServerStatus::Authenticated
        );
        assert_eq!(
            server_status(b"* bye shutting down\r\n"),
            ServerStatus::Close
        );
    }

    #[test]
    fn untagged_command_data() {
        assert_eq!(
            command_data(b"* ENABLED CONDSTORE\r\n").command_data,
            CommandData::Enabled
        );
        assert_eq!(
            command_data(b"* ID (\"name\" \"demo\")\r\n").command_data,
            CommandData::Id
        );
        assert_eq!(
            command_data(b"* LIST (\\Noselect) \"/\" \"\"\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* LSUB () \"/\" INBOX\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* NAMESPACE NIL NIL NIL\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* STATUS INBOX (MESSAGES 231)\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* SEARCH 2 84 882\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* SEARCH\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* ESEARCH (TAG \"A001\") ALL 1:3\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* FLAGS (\\Answered \\Flagged \\Deleted)\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* SORT 2 84 882\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* THREAD (2)(3 6 (4 23)(44 7 96))\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* LANGUAGE (EN)\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* COMPARATOR \"i;unicode-casemap\"\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* VANISHED 300:399\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* QUOTA \"\" (STORAGE 10 512)\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* QUOTAROOT INBOX \"\"\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* ACL INBOX user lrswipkxtecdan\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* LISTRIGHTS INBOX user l r s\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* MYRIGHTS INBOX lrswipkxtecdan\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* CONVERSION image/jpeg image/png\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* CONVERTED image/jpeg image/png\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* METADATA INBOX (/shared/comment \"Hi\")\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* GENURLAUTH imap://x\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* URLFETCH imap://x\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* 1 EXPUNGE\r\n").command_data,
            CommandData::Other
        );
        assert_eq!(
            command_data(b"* 23 RECENT\r\n").command_data,
            CommandData::Other
        );
    }

    #[test]
    fn parse_continue_line_updates_literal() {
        let mut r = command_data(b"* 12 FETCH (BODY[HEADER] {10}\r\n");
        assert_eq!(r.literal_data, Some(10));

        r.parse_continue_line(b"{20+}\r\n").unwrap();
        assert_eq!(r.literal_data, Some(20));

        r.parse_continue_line(b"\r\n").unwrap();
        assert!(r.literal_data.is_none());
    }

    #[test]
    fn malformed_lines_rejected() {
        assert!(matches!(
            Response::parse_line(b"* BYE done"),
            Err(ResponseLineError::NoTrailingSequence)
        ));
        assert!(matches!(
            Response::parse_line(b"OK done\r\n"),
            Err(ResponseLineError::NoResultField)
        ));
        assert!(matches!(
            Response::parse_line(b" A001 OK done\r\n"),
            Err(ResponseLineError::NotTagPrefixed)
        ));
        assert!(matches!(
            Response::parse_line(b"A001\r\n"),
            Err(ResponseLineError::NotTagPrefixed)
        ));
        assert!(matches!(
            Response::parse_line(b"A001 DONE completed\r\n"),
            Err(ResponseLineError::InvalidTaggedResult)
        ));
        assert!(matches!(
            Response::parse_line(b"A001 OK\r\n"),
            Err(ResponseLineError::NoResultField)
        ));
        assert!(matches!(
            Response::parse_line(b"* UNKNOWN stuff\r\n"),
            Err(ResponseLineError::UnknownUntaggedResult)
        ));
        assert!(matches!(
            Response::parse_line(b"* 12 UNKNOWN\r\n"),
            Err(ResponseLineError::UnknownUntaggedResult)
        ));
        assert!(matches!(
            Response::parse_line(b"* FOO\r\n"),
            Err(ResponseLineError::UnknownUntaggedResult)
        ));
    }

    #[test]
    fn invalid_literal_in_fetch_rejected() {
        assert!(matches!(
            Response::parse_line(b"* 12 FETCH (BODY[HEADER] {}\r\n"),
            Err(ResponseLineError::InvalidLiteralSize)
        ));
        assert!(matches!(
            Response::parse_line(b"* 12 FETCH (BODY[HEADER] {abc}\r\n"),
            Err(ResponseLineError::InvalidLiteralSize)
        ));
        assert!(matches!(
            Response::parse_line(b"* 12 FETCH (BODY[HEADER] {12x}\r\n"),
            Err(ResponseLineError::InvalidLiteralSize)
        ));
    }

    #[test]
    fn invalid_utf8_rejected() {
        assert!(matches!(
            Response::parse_line(b"A\xff01 OK done\r\n"),
            Err(ResponseLineError::InvalidUtf8Response(_))
        ));
        assert!(matches!(
            Response::parse_line(b"* CAPABILIT\xff X\r\n"),
            Err(ResponseLineError::InvalidUtf8Response(_))
        ));
    }
}
