/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::FromStr;

use anyhow::anyhow;
use clap::ArgMatches;
use humanize_rs::bytes::Bytes;

pub fn get_usize(args: &ArgMatches, id: &str) -> anyhow::Result<Option<usize>> {
    if let Some(v) = args.get_one::<String>(id) {
        // humanize-rs splits on a char count and panics inside a multibyte character.
        // A non-ASCII string is not a usize either.
        if v.is_ascii() {
            if let Ok(b) = v.parse::<Bytes>() {
                return Ok(Some(b.size()));
            }
            if let Ok(size) = usize::from_str(v) {
                return Ok(Some(size));
            }
        }
        Err(anyhow!("invalid {id} value {v}"))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Arg, ArgAction, Command};

    // Helper function to create ArgMatches for testing
    fn create_args(value: Option<&str>) -> ArgMatches {
        let arg = Arg::new("size").long("size").action(ArgAction::Set);

        let command = Command::new("test").arg(arg);

        if let Some(v) = value {
            // Use "--size=value" format to prevent negative values from being interpreted as flags
            command.get_matches_from(vec!["test", &format!("--size={v}")])
        } else {
            command.get_matches_from(vec!["test"])
        }
    }

    #[test]
    fn get_usize_none() {
        // No size provided
        let args = create_args(None);
        assert!(get_usize(&args, "size").unwrap().is_none());
    }

    #[test]
    fn get_usize_bytes_valid() {
        // Valid byte strings with various units
        let cases = vec![
            ("1", 1),
            ("1K", 1000),
            ("1k", 1000), // lowercase
            ("1KB", 1000),
            ("1M", 1000 * 1000),
            ("1G", 1000 * 1000 * 1000),
            ("1KiB", 1024),
            ("1MiB", 1024 * 1024),
            ("2 KiB", 2048),
        ];

        for (input, expected) in cases {
            let args = create_args(Some(input));
            assert_eq!(
                get_usize(&args, "size").unwrap(),
                Some(expected),
                "input {input}"
            );
        }
    }

    #[test]
    fn get_usize_number_valid() {
        // Valid number strings
        let cases = vec![
            ("0", 0),
            ("1024", 1024),
            ("65535", 65535),
            ("18446744073709551615", usize::MAX), // max usize
        ];

        for (input, expected) in cases {
            let args = create_args(Some(input));
            assert_eq!(get_usize(&args, "size").unwrap(), Some(expected));
        }
    }

    #[test]
    fn get_usize_invalid() {
        // Non-ASCII is rejected before parse. `1·K`, `100°C`, and `１K` used to panic.
        let cases = [
            "abc",
            "1.2.3",
            "-5",
            "123a",
            "1·K",
            "100°C",
            "１K",
            "·1K",
            "1Ｋ",
            "1\u{00A0}KiB",
        ];

        for input in cases {
            let args = create_args(Some(input));
            assert!(get_usize(&args, "size").is_err(), "{input}");
        }
    }
}
