/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fmt;

#[derive(Debug, Eq, PartialEq)]
pub enum FtpFileEntryType {
    Unknown,
    File,
    Directory,
    CurrentDir,
    ParentDir,
    OsType(String),
}

impl fmt::Display for FtpFileEntryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FtpFileEntryType {
    pub(super) fn parse(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "file" => FtpFileEntryType::File,
            "dir" => FtpFileEntryType::Directory,
            "cdir" => FtpFileEntryType::CurrentDir,
            "pdir" => FtpFileEntryType::ParentDir,
            _ => FtpFileEntryType::OsType(value.to_owned()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            FtpFileEntryType::Unknown => "unknown",
            FtpFileEntryType::File => "file",
            FtpFileEntryType::Directory => "dir",
            FtpFileEntryType::CurrentDir => "cdir",
            FtpFileEntryType::ParentDir => "pdir",
            FtpFileEntryType::OsType(s) => s,
        }
    }

    pub fn is_dir(&self) -> bool {
        matches!(
            self,
            FtpFileEntryType::Directory
                | FtpFileEntryType::CurrentDir
                | FtpFileEntryType::ParentDir
        )
    }

    pub fn maybe_file(&self) -> bool {
        match self {
            FtpFileEntryType::Unknown => true,
            FtpFileEntryType::File => true,
            FtpFileEntryType::Directory => false,
            FtpFileEntryType::CurrentDir => false,
            FtpFileEntryType::ParentDir => false,
            FtpFileEntryType::OsType(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_known_types() {
        assert_eq!(FtpFileEntryType::parse("file"), FtpFileEntryType::File);
        assert_eq!(FtpFileEntryType::parse("DIR"), FtpFileEntryType::Directory);
        assert_eq!(
            FtpFileEntryType::parse("cdir"),
            FtpFileEntryType::CurrentDir
        );
        assert_eq!(FtpFileEntryType::parse("pdir"), FtpFileEntryType::ParentDir);
    }

    #[test]
    fn parse_os_specific_type() {
        let t = FtpFileEntryType::parse("unix.slink");
        assert_eq!(t.as_str(), "unix.slink");
        assert!(t.maybe_file());
        assert!(!t.is_dir());
    }

    #[test]
    fn display_roundtrip() {
        for t in [
            FtpFileEntryType::File,
            FtpFileEntryType::Directory,
            FtpFileEntryType::CurrentDir,
            FtpFileEntryType::ParentDir,
        ] {
            assert_eq!(format!("{t}"), t.as_str());
        }
    }
}
