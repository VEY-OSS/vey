/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::FromStr;

use jiff::Timestamp;
use mime::Mime;

use crate::error::FtpFileFactsParseError;

mod entry_type;
pub(crate) mod time_val;

pub use entry_type::FtpFileEntryType;

pub struct FtpFileFacts {
    entry_path: String,
    entry_type: FtpFileEntryType,
    size: Option<u64>,
    media_type: Option<Mime>,
    modify_time: Option<Timestamp>,
    create_time: Option<Timestamp>,
}

impl FtpFileFacts {
    pub(crate) fn new(path: &str) -> Self {
        FtpFileFacts {
            entry_path: path.to_owned(),
            entry_type: FtpFileEntryType::Unknown,
            size: None,
            media_type: None,
            modify_time: None,
            create_time: None,
        }
    }

    #[inline]
    pub fn entry_path(&self) -> &str {
        self.entry_path.as_str()
    }

    #[inline]
    pub fn entry_type(&self) -> &FtpFileEntryType {
        &self.entry_type
    }

    #[inline]
    pub fn maybe_file(&self) -> bool {
        self.entry_type.maybe_file()
    }

    #[inline]
    pub fn size(&self) -> Option<u64> {
        self.size
    }

    #[inline]
    pub(crate) fn set_size(&mut self, size: u64) {
        self.size = Some(size);
    }

    #[inline]
    pub fn mtime(&self) -> Option<&Timestamp> {
        self.modify_time.as_ref()
    }

    #[inline]
    pub(crate) fn set_mtime(&mut self, mtime: Timestamp) {
        self.modify_time = Some(mtime);
    }

    #[inline]
    pub fn media_type(&self) -> Option<&Mime> {
        self.media_type.as_ref()
    }

    pub(crate) fn parse_line(line: &str) -> Result<Self, FtpFileFactsParseError> {
        if let Some((facts, path)) = line.trim_start().split_once(' ') {
            let mut ff = FtpFileFacts::new(path);

            for fact in facts.split(';') {
                if fact.is_empty() {
                    continue;
                }

                if let Some((key, value)) = fact.split_once('=') {
                    ff.set_fact(key, value)?;
                } else {
                    return Err(FtpFileFactsParseError::NoDelimiterInFact(fact.to_owned()));
                }
            }

            Ok(ff)
        } else {
            Err(FtpFileFactsParseError::NoSpaceDelimiter)
        }
    }

    fn set_fact(&mut self, key: &str, value: &str) -> Result<(), FtpFileFactsParseError> {
        match key.to_lowercase().as_str() {
            "type" => self.entry_type = FtpFileEntryType::parse(value),
            "modify" => {
                let dt = time_val::parse_from_str(value)
                    .map_err(FtpFileFactsParseError::InvalidModifyTime)?;
                self.modify_time = Some(dt);
            }
            "create" => {
                let dt = time_val::parse_from_str(value)
                    .map_err(FtpFileFactsParseError::InvalidCreateTime)?;
                self.create_time = Some(dt);
            }
            "size" => {
                let size = u64::from_str(value).map_err(|_| FtpFileFactsParseError::InvalidSize)?;
                self.size = Some(size);
            }
            "media-type" => {
                if let Ok(mime) = value.parse() {
                    self.media_type = Some(mime);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_line() {
        let ff = FtpFileFacts::parse_line("type=pdir;sizd=4096;modify=20210525083610;UNIX.mode=0755;UNIX.uid=0;UNIX.gid=0;unique=804g2; /").unwrap();
        assert_eq!(ff.entry_type, FtpFileEntryType::ParentDir);
        assert_eq!(ff.entry_path(), "/");
        assert!(ff.size.is_none());
        assert!(!ff.maybe_file());
    }

    #[test]
    fn parse_line_with_common_facts() {
        let ff = FtpFileFacts::parse_line(
            "type=file;size=1024;modify=20211201102030;create=20211101000000;media-type=text/plain; /docs/readme.txt",
        )
        .unwrap();
        assert_eq!(ff.entry_type(), &FtpFileEntryType::File);
        assert_eq!(ff.entry_path(), "/docs/readme.txt");
        assert_eq!(ff.size(), Some(1024));
        assert!(ff.maybe_file());
        assert_eq!(ff.mtime().unwrap().to_string(), "2021-12-01T10:20:30Z");
        assert_eq!(
            ff.create_time.as_ref().unwrap().to_string(),
            "2021-11-01T00:00:00Z"
        );
        assert_eq!(ff.media_type().unwrap().essence_str(), "text/plain");
    }

    #[test]
    fn parse_line_skips_empty_facts_and_unknown_keys() {
        let ff = FtpFileFacts::parse_line("type=file;;perm=r; /a").unwrap();
        assert_eq!(ff.entry_type(), &FtpFileEntryType::File);
        assert_eq!(ff.entry_path(), "/a");
        assert!(ff.size().is_none());
    }

    #[test]
    fn parse_line_ignores_invalid_media_type() {
        let ff = FtpFileFacts::parse_line("type=file;media-type=@@@; /a").unwrap();
        assert!(ff.media_type().is_none());
    }

    #[test]
    fn parse_line_errors() {
        assert!(matches!(
            FtpFileFacts::parse_line("nospace"),
            Err(FtpFileFactsParseError::NoSpaceDelimiter)
        ));
        assert!(matches!(
            FtpFileFacts::parse_line("typefile; /a"),
            Err(FtpFileFactsParseError::NoDelimiterInFact(_))
        ));
        assert!(matches!(
            FtpFileFacts::parse_line("size=abc; /a"),
            Err(FtpFileFactsParseError::InvalidSize)
        ));
        assert!(matches!(
            FtpFileFacts::parse_line("modify=not-a-time; /a"),
            Err(FtpFileFactsParseError::InvalidModifyTime(_))
        ));
        assert!(matches!(
            FtpFileFacts::parse_line("create=not-a-time; /a"),
            Err(FtpFileFactsParseError::InvalidCreateTime(_))
        ));
    }

    #[test]
    fn set_size_and_mtime() {
        let mut ff = FtpFileFacts::new("/tmp/x");
        assert_eq!(ff.entry_path(), "/tmp/x");
        assert!(ff.maybe_file());
        ff.set_size(9);
        assert_eq!(ff.size(), Some(9));
        let dt = time_val::parse_from_str("20211201102030").unwrap();
        ff.set_mtime(dt);
        assert_eq!(ff.mtime().unwrap(), &dt);
    }
}
