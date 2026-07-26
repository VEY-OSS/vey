/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use clap::ArgMatches;

pub fn get(args: &ArgMatches, id: &str, decode_binary: bool) -> anyhow::Result<Vec<u8>> {
    let Some(s) = args.get_one::<String>(id) else {
        return Ok(Vec::new());
    };

    let raw = if let Some(p) = s.strip_prefix('@') {
        std::fs::read(p).map_err(|e| anyhow!("failed to read content from file {p}: {e}"))?
    } else if let Some(name) = s.strip_prefix('$') {
        std::env::var(name)
            .map(|s| s.into_bytes())
            .map_err(|e| anyhow!("failed to read environment variable {name}: {e}"))?
    } else {
        s.as_bytes().to_vec()
    };

    if decode_binary {
        hex::decode(raw).map_err(|e| anyhow!("not valid hex encoded request struct: {e}"))
    } else {
        Ok(raw)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use clap::{Arg, ArgAction, Command};

    use super::get;

    fn create_args(value: Option<&str>) -> clap::ArgMatches {
        let command = Command::new("test").arg(Arg::new("data").long("data").action(ArgAction::Set));
        if let Some(v) = value {
            command.get_matches_from(vec!["test", &format!("--data={v}")])
        } else {
            command.get_matches_from(vec!["test"])
        }
    }

    fn unique_path(prefix: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("vey-clap-{prefix}-{nanos}-{n}"))
    }

    #[test]
    fn get_missing_returns_empty() {
        let args = create_args(None);
        assert!(get(&args, "data", false).unwrap().is_empty());
        assert!(get(&args, "data", true).unwrap().is_empty());
    }

    #[test]
    fn get_literal_text_and_hex() {
        let args = create_args(Some("hello"));
        assert_eq!(get(&args, "data", false).unwrap(), b"hello");

        let args = create_args(Some("68656c6c6f"));
        assert_eq!(get(&args, "data", true).unwrap(), b"hello");

        let args = create_args(Some("not-hex"));
        assert!(get(&args, "data", true).is_err());
    }

    #[test]
    fn get_from_file() {
        let path = unique_path("file");
        fs::write(&path, b"from-file").unwrap();
        let arg = format!("@{}", path.display());
        let args = create_args(Some(&arg));
        assert_eq!(get(&args, "data", false).unwrap(), b"from-file");

        fs::write(&path, b"deadbeef").unwrap();
        let args = create_args(Some(&arg));
        assert_eq!(get(&args, "data", true).unwrap(), [0xde, 0xad, 0xbe, 0xef]);

        let args = create_args(Some("@/definitely/missing/vey-clap-data.bin"));
        assert!(get(&args, "data", false).is_err());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn get_from_env() {
        let key = format!(
            "VEY_CLAP_TEST_DATA_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        // SAFETY: unique key only used in this test process.
        unsafe { std::env::set_var(&key, "env-value") };

        let arg = format!("${key}");
        let args = create_args(Some(&arg));
        assert_eq!(get(&args, "data", false).unwrap(), b"env-value");

        unsafe { std::env::set_var(&key, "616263") };
        let args = create_args(Some(&arg));
        assert_eq!(get(&args, "data", true).unwrap(), b"abc");

        let args = create_args(Some("$VEY_CLAP_TEST_DATA_MISSING_XYZ"));
        assert!(get(&args, "data", false).is_err());

        unsafe { std::env::remove_var(&key) };
    }
}
