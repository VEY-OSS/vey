/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::num::NonZeroU32;
use std::str::FromStr;
use std::time::Duration;

use anyhow::anyhow;
use clap::ArgMatches;

use vey_types::limit::RateLimitQuota;

pub fn get_rate_limit(args: &ArgMatches, id: &str) -> anyhow::Result<Option<RateLimitQuota>> {
    let Some(v) = args.get_one::<String>(id) else {
        return Ok(None);
    };

    let quota = if let Some((v1, v2)) = v.split_once('/') {
        let cells =
            NonZeroU32::from_str(v1.trim()).map_err(|e| anyhow!("invalid cells value: {e}"))?;
        let interval_s = v2.trim();
        if let Ok(seconds) = u64::from_str(interval_s) {
            RateLimitQuota::new(Duration::from_secs(seconds), cells)?
        } else if let Ok(interval) = humanize_rs::duration::parse(interval_s) {
            RateLimitQuota::new(interval, cells)?
        } else {
            match interval_s {
                "s" => RateLimitQuota::per_second(cells)?,
                "m" => RateLimitQuota::per_minute(cells)?,
                "h" => RateLimitQuota::per_hour(cells)?,
                _ => return Err(anyhow!("invalid interval value {v2}")),
            }
        }
    } else {
        let cells = NonZeroU32::from_str(v).map_err(|e| anyhow!("invalid cells value: {e}"))?;
        RateLimitQuota::per_second(cells)?
    };
    Ok(Some(quota))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;
    use std::time::Duration;

    use clap::{Arg, ArgAction, Command};
    use vey_types::limit::RateLimitQuota;

    use super::get_rate_limit;

    fn create_args(value: Option<&str>) -> clap::ArgMatches {
        let command =
            Command::new("test").arg(Arg::new("rate").long("rate").action(ArgAction::Set));
        if let Some(v) = value {
            command.get_matches_from(vec!["test", &format!("--rate={v}")])
        } else {
            command.get_matches_from(vec!["test"])
        }
    }

    fn nz(v: u32) -> NonZeroU32 {
        NonZeroU32::new(v).unwrap()
    }

    #[test]
    fn get_rate_limit_none() {
        let args = create_args(None);
        assert!(get_rate_limit(&args, "rate").unwrap().is_none());
    }

    #[test]
    fn get_rate_limit_ok() {
        let args = create_args(Some("10"));
        assert_eq!(
            get_rate_limit(&args, "rate").unwrap(),
            Some(RateLimitQuota::per_second(nz(10)).unwrap())
        );

        let args = create_args(Some("10/s"));
        assert_eq!(
            get_rate_limit(&args, "rate").unwrap(),
            Some(RateLimitQuota::per_second(nz(10)).unwrap())
        );

        let args = create_args(Some("10/m"));
        assert_eq!(
            get_rate_limit(&args, "rate").unwrap(),
            Some(RateLimitQuota::per_minute(nz(10)).unwrap())
        );

        let args = create_args(Some("10/h"));
        assert_eq!(
            get_rate_limit(&args, "rate").unwrap(),
            Some(RateLimitQuota::per_hour(nz(10)).unwrap())
        );

        let args = create_args(Some("5/2"));
        assert_eq!(
            get_rate_limit(&args, "rate").unwrap(),
            Some(RateLimitQuota::new(Duration::from_secs(2), nz(5)).unwrap())
        );

        let args = create_args(Some("5/2s"));
        assert_eq!(
            get_rate_limit(&args, "rate").unwrap(),
            Some(RateLimitQuota::new(Duration::from_secs(2), nz(5)).unwrap())
        );

        let args = create_args(Some(" 8 / s "));
        assert_eq!(
            get_rate_limit(&args, "rate").unwrap(),
            Some(RateLimitQuota::per_second(nz(8)).unwrap())
        );
    }

    #[test]
    fn get_rate_limit_err() {
        for input in ["0", "abc", "10/x", "10/0", "-1/s", ""] {
            let args = create_args(Some(input));
            assert!(
                get_rate_limit(&args, "rate").is_err(),
                "expected error for {input:?}"
            );
        }
    }
}
