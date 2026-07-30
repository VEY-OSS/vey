/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fmt;
use std::str::{self, Utf8Error};

use atoi::FromRadix10Checked;
use log::trace;
use smol_str::SmolStr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CommandLineError {
    #[error("no trailing sequence")]
    NoTrailingSequence,
    #[error("no tag found as a prefix")]
    NotTagPrefixed,
    #[error("invalid utf-8 command: {0}")]
    InvalidUtf8Command(Utf8Error),
    #[error("invalid literal format")]
    InvalidLiteralFormat,
    #[error("invalid literal size")]
    InvalidLiteralSize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParsedCommand {
    Capability,
    NoOperation,
    Logout,
    StartTls,
    Auth,
    Login,
    Enable,
    Select,
    Examine,
    Create,
    Delete,
    Rename,
    Subscribe,
    Unsubscribe,
    List,
    Lsub,      // rev1
    Namespace, // rfc2342, rev2
    Status,
    Append,
    Idle, // rfc2177, rev2
    Close,
    Unselect, // rfc3691, rev2
    Expunge,
    Search,
    Fetch,
    Store,
    Copy,
    Move, // rfc6851, rev2
    Uid,
    Id,           // rfc2971, rev2
    CancelUpdate, // rfc5267
    Sort,         // rfc5256
    Thread,       // rfc5256
    Language,     // rfc5255
    Comparator,   // rfc5255
    Esearch,
    GetQuota,       // rfc9208
    GetQuotaRoot,   // rfc9208
    SetQuota,       // rfc9208
    SetAcl,         // rfc4314
    DeleteAcl,      // rfc4314
    GetAcl,         // rfc4314
    ListRights,     // rfc4314
    MyRights,       // rfc4314
    Conversions,    // rfc5259
    Convert,        // rfc5259
    SetMetadata,    // rfc5464
    GetMetadata,    // rfc5464
    Notify,         // rfc5465
    UnAuthenticate, // rfc8437
    ResetKey,       // rfc4467
    GenUrlAuth,     // rfc4467
    UrlFetch,       // rfc4467
    Unknown,
}

#[derive(Clone, Copy)]
pub struct LiteralArgument {
    pub size: u64,
    pub wait_continuation: bool,
}

impl LiteralArgument {
    fn parse_size(buf: &[u8]) -> Result<Self, CommandLineError> {
        if buf.is_empty() {
            return Err(CommandLineError::InvalidLiteralFormat);
        }
        let (size, offset) = u64::from_radix_10_checked(buf);
        let Some(size) = size else {
            return Err(CommandLineError::InvalidLiteralSize);
        };
        if offset == 0 {
            return Err(CommandLineError::InvalidLiteralFormat);
        } else if offset == buf.len() {
            return Ok(LiteralArgument {
                size,
                wait_continuation: true,
            });
        } else if offset + 1 == buf.len() && buf[offset] == b'+' {
            return Ok(LiteralArgument {
                size,
                wait_continuation: false,
            });
        }

        Err(CommandLineError::InvalidLiteralFormat)
    }

    fn check(left: &[u8]) -> Result<Option<Self>, CommandLineError> {
        if left.ends_with(b"}")
            && let Some(p) = memchr::memrchr(b'{', left)
        {
            let arg = Self::parse_size(&left[p + 1..left.len() - 1])?;
            return Ok(Some(arg));
        }
        Ok(None)
    }
}

pub struct Command {
    pub tag: SmolStr,
    pub parsed: ParsedCommand,
    pub literal_arg: Option<LiteralArgument>,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}/{}", self.parsed, self.tag)
    }
}

impl Command {
    pub fn parse_line(line: &[u8]) -> Result<Self, CommandLineError> {
        let left = line
            .strip_suffix(b"\r\n")
            .ok_or(CommandLineError::NoTrailingSequence)?;

        #[cfg(debug_assertions)]
        if let Ok(s) = str::from_utf8(left) {
            trace!("[IMAP] --> {s}");
        }

        let Some(d) = memchr::memchr(b' ', left) else {
            return Err(CommandLineError::NotTagPrefixed);
        };

        let tag = str::from_utf8(&left[..d]).map_err(CommandLineError::InvalidUtf8Command)?;
        let left = &left[d + 1..];
        if left.is_empty() {
            return Err(CommandLineError::NotTagPrefixed);
        }

        if let Some(p) = memchr::memchr(b' ', left) {
            // commands with params
            let cmd = str::from_utf8(&left[0..p]).map_err(CommandLineError::InvalidUtf8Command)?;
            let upper_cmd = cmd.to_uppercase();

            let left = &left[p + 1..];
            let literal_arg = LiteralArgument::check(left)?;
            let parsed = match upper_cmd.as_bytes() {
                b"AUTHENTICATE" => ParsedCommand::Auth,
                b"LOGIN" => ParsedCommand::Login, // TODO parse username
                b"ENABLE" => ParsedCommand::Enable,
                b"SELECT" => ParsedCommand::Select,
                b"EXAMINE" => ParsedCommand::Examine,
                b"CREATE" => ParsedCommand::Create,
                b"DELETE" => ParsedCommand::Delete,
                b"RENAME" => ParsedCommand::Rename,
                b"SUBSCRIBE" => ParsedCommand::Subscribe,
                b"UNSUBSCRIBE" => ParsedCommand::Unsubscribe,
                b"LIST" => ParsedCommand::List,
                b"LSUB" => ParsedCommand::Lsub,
                b"STATUS" => ParsedCommand::Status,
                b"APPEND" => ParsedCommand::Append,
                b"SEARCH" => ParsedCommand::Search,
                b"FETCH" => ParsedCommand::Fetch,
                b"STORE" => ParsedCommand::Store,
                b"COPY" => ParsedCommand::Copy,
                b"MOVE" => ParsedCommand::Move,
                b"UID" => ParsedCommand::Uid,
                b"ID" => ParsedCommand::Id,
                b"CANCELUPDATE" => ParsedCommand::CancelUpdate,
                b"SORT" => ParsedCommand::Sort,
                b"THREAD" => ParsedCommand::Thread,
                b"LANGUAGE" => ParsedCommand::Language,
                b"COMPARATOR" => ParsedCommand::Comparator,
                b"ESEARCH" => ParsedCommand::Esearch,
                b"GETQUOTA" => ParsedCommand::GetQuota,
                b"GETQUOTAROOT" => ParsedCommand::GetQuotaRoot,
                b"SETQUOTA" => ParsedCommand::SetQuota,
                b"GETACL" => ParsedCommand::GetAcl,
                b"DELETEACL" => ParsedCommand::DeleteAcl,
                b"SETACL" => ParsedCommand::SetAcl,
                b"LISTRIGHTS" => ParsedCommand::ListRights,
                b"MYRIGHTS" => ParsedCommand::MyRights,
                b"CONVERSIONS" => ParsedCommand::Conversions,
                b"CONVERT" => ParsedCommand::Convert,
                b"GETMETADATA" => ParsedCommand::GetMetadata,
                b"SETMETADATA" => ParsedCommand::SetMetadata,
                b"NOTIFY" => ParsedCommand::Notify,
                b"RESETKEY" => ParsedCommand::ResetKey,
                b"GENURLAUTH" => ParsedCommand::GenUrlAuth,
                b"URLFETCH" => ParsedCommand::UrlFetch,
                _ => {
                    trace!("unknown IMAP command: {tag} {upper_cmd} ...");
                    ParsedCommand::Unknown
                }
            };

            Ok(Command {
                tag: SmolStr::from(tag),
                parsed,
                literal_arg,
            })
        } else {
            // commands without params
            let cmd = str::from_utf8(left).map_err(CommandLineError::InvalidUtf8Command)?;
            let upper_cmd = cmd.to_uppercase();

            let data = match upper_cmd.as_bytes() {
                b"CAPABILITY" => ParsedCommand::Capability,
                b"NOOP" => ParsedCommand::NoOperation,
                b"LOGOUT" => ParsedCommand::Logout,
                b"STARTTLS" => ParsedCommand::StartTls,
                b"NAMESPACE" => ParsedCommand::Namespace,
                b"IDLE" => ParsedCommand::Idle,
                b"CLOSE" => ParsedCommand::Close,
                b"UNSELECT" => ParsedCommand::Unselect,
                b"EXPUNGE" => ParsedCommand::Expunge,
                b"LANGUAGE" => ParsedCommand::Language,
                b"COMPARATOR" => ParsedCommand::Comparator,
                b"UNAUTHENTICATE" => ParsedCommand::UnAuthenticate,
                b"RESETKEY" => ParsedCommand::ResetKey,
                _ => {
                    trace!("unknown IMAP command: {tag} {upper_cmd}");
                    ParsedCommand::Unknown
                }
            };

            Ok(Command {
                tag: SmolStr::from(tag),
                parsed: data,
                literal_arg: None,
            })
        }
    }

    pub fn parse_continue_line(&mut self, line: &[u8]) -> Result<(), CommandLineError> {
        let left = line
            .strip_suffix(b"\r\n")
            .ok_or(CommandLineError::NoTrailingSequence)?;

        #[cfg(debug_assertions)]
        if let Ok(s) = str::from_utf8(left) {
            trace!("[IMAP] +-> {s}");
        }

        if left.is_empty() {
            self.literal_arg = None;
        } else {
            self.literal_arg = LiteralArgument::check(left)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &[u8]) -> Command {
        Command::parse_line(line).unwrap()
    }

    fn assert_parsed(line: &[u8], expected: ParsedCommand) -> Command {
        let cmd = parse(line);
        assert_eq!(cmd.parsed, expected);
        cmd
    }

    #[test]
    fn capability() {
        let cmd = parse(b"a441 CAPABILITY\r\n");
        assert_eq!(cmd.tag.as_str(), "a441");
        assert_eq!(cmd.parsed, ParsedCommand::Capability);
        assert!(cmd.literal_arg.is_none());
    }

    #[test]
    fn append() {
        let cmd = parse(b"A003 APPEND saved-messages (\\Seen) {326}\r\n");
        assert_eq!(cmd.tag.as_str(), "A003");
        assert_eq!(cmd.parsed, ParsedCommand::Append);
        let literal = cmd.literal_arg.unwrap();
        assert!(literal.wait_continuation);
        assert_eq!(literal.size, 326);

        let cmd = parse(b"A003 APPEND saved-messages (\\Seen) {297+}\r\n");
        assert_eq!(cmd.tag.as_str(), "A003");
        assert_eq!(cmd.parsed, ParsedCommand::Append);
        let literal = cmd.literal_arg.unwrap();
        assert!(!literal.wait_continuation);
        assert_eq!(literal.size, 297);
    }

    #[test]
    fn enable() {
        let cmd = parse(b"A001 ENABLE CONDSTORE\r\n");
        assert_eq!(cmd.tag.as_str(), "A001");
        assert_eq!(cmd.parsed, ParsedCommand::Enable);
        assert!(cmd.literal_arg.is_none());
    }

    #[test]
    fn login() {
        assert_parsed(b"A002 LOGIN user pass\r\n", ParsedCommand::Login);
    }

    #[test]
    fn noop() {
        assert_parsed(b"A003 NOOP\r\n", ParsedCommand::NoOperation);
    }

    #[test]
    fn commands_without_params() {
        assert_parsed(b"A001 LOGOUT\r\n", ParsedCommand::Logout);
        assert_parsed(b"A002 STARTTLS\r\n", ParsedCommand::StartTls);
        assert_parsed(b"A003 NAMESPACE\r\n", ParsedCommand::Namespace);
        assert_parsed(b"A004 IDLE\r\n", ParsedCommand::Idle);
        assert_parsed(b"A005 CLOSE\r\n", ParsedCommand::Close);
        assert_parsed(b"A006 UNSELECT\r\n", ParsedCommand::Unselect);
        assert_parsed(b"A007 EXPUNGE\r\n", ParsedCommand::Expunge);
        assert_parsed(b"A008 LANGUAGE\r\n", ParsedCommand::Language);
        assert_parsed(b"A009 COMPARATOR\r\n", ParsedCommand::Comparator);
        assert_parsed(b"A010 UNAUTHENTICATE\r\n", ParsedCommand::UnAuthenticate);
        assert_parsed(b"A011 RESETKEY\r\n", ParsedCommand::ResetKey);
        // CONVERSIONS requires source/target MIME args (RFC 5259); bare form is Unknown
        // so the proxy can reply tagged BAD rather than forwarding.
        assert_parsed(b"A012 CONVERSIONS\r\n", ParsedCommand::Unknown);
    }

    #[test]
    fn commands_with_params() {
        assert_parsed(b"A001 AUTHENTICATE PLAIN\r\n", ParsedCommand::Auth);
        assert_parsed(b"A002 SELECT INBOX\r\n", ParsedCommand::Select);
        assert_parsed(b"A003 EXAMINE INBOX\r\n", ParsedCommand::Examine);
        assert_parsed(b"A004 CREATE foo\r\n", ParsedCommand::Create);
        assert_parsed(b"A005 DELETE foo\r\n", ParsedCommand::Delete);
        assert_parsed(b"A006 RENAME foo bar\r\n", ParsedCommand::Rename);
        assert_parsed(b"A007 SUBSCRIBE foo\r\n", ParsedCommand::Subscribe);
        assert_parsed(b"A008 UNSUBSCRIBE foo\r\n", ParsedCommand::Unsubscribe);
        assert_parsed(b"A009 LIST \"\" *\r\n", ParsedCommand::List);
        assert_parsed(b"A010 LSUB \"\" *\r\n", ParsedCommand::Lsub);
        assert_parsed(b"A011 STATUS INBOX (MESSAGES)\r\n", ParsedCommand::Status);
        assert_parsed(b"A012 SEARCH ALL\r\n", ParsedCommand::Search);
        assert_parsed(b"A013 FETCH 1:* FLAGS\r\n", ParsedCommand::Fetch);
        assert_parsed(b"A014 STORE 1 +FLAGS (\\Seen)\r\n", ParsedCommand::Store);
        assert_parsed(b"A015 COPY 1:3 archive\r\n", ParsedCommand::Copy);
        assert_parsed(b"A016 MOVE 1:3 archive\r\n", ParsedCommand::Move);
        assert_parsed(b"A017 UID FETCH 1:* FLAGS\r\n", ParsedCommand::Uid);
        assert_parsed(b"A018 ID NIL\r\n", ParsedCommand::Id);
        assert_parsed(b"A019 CANCELUPDATE 1\r\n", ParsedCommand::CancelUpdate);
        assert_parsed(b"A020 SORT (DATE) UTF-8 ALL\r\n", ParsedCommand::Sort);
        assert_parsed(
            b"A021 THREAD REFERENCES UTF-8 ALL\r\n",
            ParsedCommand::Thread,
        );
        assert_parsed(b"A022 LANGUAGE en\r\n", ParsedCommand::Language);
        assert_parsed(
            b"A023 COMPARATOR \"i;unicode-casemap\"\r\n",
            ParsedCommand::Comparator,
        );
        assert_parsed(
            b"A024 ESEARCH IN (mailboxes) RETURN (ALL) ALL\r\n",
            ParsedCommand::Esearch,
        );
        assert_parsed(b"A025 GETQUOTA \"\"\r\n", ParsedCommand::GetQuota);
        assert_parsed(b"A026 GETQUOTAROOT INBOX\r\n", ParsedCommand::GetQuotaRoot);
        assert_parsed(
            b"A027 SETQUOTA \"\" (STORAGE 512)\r\n",
            ParsedCommand::SetQuota,
        );
        assert_parsed(b"A028 GETACL INBOX\r\n", ParsedCommand::GetAcl);
        assert_parsed(b"A029 DELETEACL INBOX user\r\n", ParsedCommand::DeleteAcl);
        assert_parsed(
            b"A030 SETACL INBOX user lrswipkxtecdan\r\n",
            ParsedCommand::SetAcl,
        );
        assert_parsed(b"A031 LISTRIGHTS INBOX user\r\n", ParsedCommand::ListRights);
        assert_parsed(b"A032 MYRIGHTS INBOX\r\n", ParsedCommand::MyRights);
        assert_parsed(
            b"A033 CONVERSIONS image/jpeg\r\n",
            ParsedCommand::Conversions,
        );
        assert_parsed(
            b"A034 CONVERT 1 image/jpeg image/png\r\n",
            ParsedCommand::Convert,
        );
        assert_parsed(
            b"A035 GETMETADATA INBOX (/shared)\r\n",
            ParsedCommand::GetMetadata,
        );
        assert_parsed(
            b"A036 SETMETADATA INBOX (/shared \"x\")\r\n",
            ParsedCommand::SetMetadata,
        );
        assert_parsed(b"A037 NOTIFY NONE\r\n", ParsedCommand::Notify);
        assert_parsed(b"A038 RESETKEY INBOX\r\n", ParsedCommand::ResetKey);
        assert_parsed(
            b"A039 GENURLAUTH imap://x INTERNAL\r\n",
            ParsedCommand::GenUrlAuth,
        );
        assert_parsed(b"A040 URLFETCH imap://x\r\n", ParsedCommand::UrlFetch);
    }

    #[test]
    fn command_names_are_case_insensitive() {
        assert_parsed(b"a1 capability\r\n", ParsedCommand::Capability);
        assert_parsed(b"a2 NoOp\r\n", ParsedCommand::NoOperation);
        assert_parsed(b"a3 login USER PASS\r\n", ParsedCommand::Login);
        assert_parsed(b"a4 UnSubscribe foo\r\n", ParsedCommand::Unsubscribe);
        assert_parsed(b"a5 eNaBlE CONDSTORE\r\n", ParsedCommand::Enable);
    }

    #[test]
    fn unknown_commands() {
        assert_parsed(b"A001 FOOBAR\r\n", ParsedCommand::Unknown);
        assert_parsed(b"A002 FOOBAR arg\r\n", ParsedCommand::Unknown);
    }

    #[test]
    fn display_includes_parsed_and_tag() {
        let cmd = parse(b"A001 NOOP\r\n");
        assert_eq!(cmd.to_string(), "NoOperation/A001");
    }

    #[test]
    fn parse_continue_line_updates_literal() {
        let mut cmd = parse(b"A003 APPEND m () {10}\r\n");
        assert_eq!(cmd.literal_arg.unwrap().size, 10);

        cmd.parse_continue_line(b"{20+}\r\n").unwrap();
        let literal = cmd.literal_arg.unwrap();
        assert_eq!(literal.size, 20);
        assert!(!literal.wait_continuation);

        cmd.parse_continue_line(b"\r\n").unwrap();
        assert!(cmd.literal_arg.is_none());
    }

    #[test]
    fn missing_crlf_rejected() {
        assert!(matches!(
            Command::parse_line(b"A003 NOOP"),
            Err(CommandLineError::NoTrailingSequence)
        ));
    }

    #[test]
    fn missing_tag_rejected() {
        assert!(matches!(
            Command::parse_line(b"NOOP\r\n"),
            Err(CommandLineError::NotTagPrefixed)
        ));
        assert!(matches!(
            Command::parse_line(b"A001 \r\n"),
            Err(CommandLineError::NotTagPrefixed)
        ));
        assert!(matches!(
            Command::parse_line(b"\r\n"),
            Err(CommandLineError::NotTagPrefixed)
        ));
    }

    #[test]
    fn invalid_utf8_tag_or_command_rejected() {
        assert!(matches!(
            Command::parse_line(b"A\xff01 NOOP\r\n"),
            Err(CommandLineError::InvalidUtf8Command(_))
        ));
        assert!(matches!(
            Command::parse_line(b"A001 N\xffOP\r\n"),
            Err(CommandLineError::InvalidUtf8Command(_))
        ));
        assert!(matches!(
            Command::parse_line(b"A001 L\xffGIN user\r\n"),
            Err(CommandLineError::InvalidUtf8Command(_))
        ));
    }

    #[test]
    fn invalid_literal_size() {
        assert!(matches!(
            Command::parse_line(b"A003 APPEND m () {abc}\r\n"),
            Err(CommandLineError::InvalidLiteralFormat)
        ));
        assert!(matches!(
            Command::parse_line(b"A003 APPEND m () {}\r\n"),
            Err(CommandLineError::InvalidLiteralFormat)
        ));
        assert!(matches!(
            Command::parse_line(b"A003 APPEND m () {12x}\r\n"),
            Err(CommandLineError::InvalidLiteralFormat)
        ));
        assert!(matches!(
            Command::parse_line(b"A003 APPEND m () {12+x}\r\n"),
            Err(CommandLineError::InvalidLiteralFormat)
        ));
    }

    #[test]
    fn continue_line_rejects_bad_input() {
        let mut cmd = parse(b"A003 APPEND m () {10}\r\n");
        assert!(matches!(
            cmd.parse_continue_line(b"{10}"),
            Err(CommandLineError::NoTrailingSequence)
        ));
        assert!(matches!(
            cmd.parse_continue_line(b"{abc}\r\n"),
            Err(CommandLineError::InvalidLiteralFormat)
        ));
    }
}
