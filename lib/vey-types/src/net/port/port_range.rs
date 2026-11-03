/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::num::NonZeroU16;
use std::str::FromStr;

use anyhow::anyhow;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct PortRange {
    start: NonZeroU16,
    end: NonZeroU16,
}

impl PortRange {
    pub fn new(port_start: NonZeroU16, port_end: NonZeroU16) -> Self {
        PortRange {
            start: port_start,
            end: port_end,
        }
    }

    /// Number of ports in this range.
    ///
    /// Every value is a valid port, so the largest range (`1-65535`) contains
    /// 65535 ports and fits in `u16`.
    #[inline]
    pub fn count(&self) -> u16 {
        self.end.get() - self.start.get() + 1
    }

    #[inline]
    pub fn start(&self) -> u16 {
        self.start.get()
    }

    #[inline]
    pub fn end(&self) -> u16 {
        self.end.get()
    }

    pub fn check(&self) -> anyhow::Result<()> {
        if self.end.get() <= self.start.get() {
            return Err(anyhow!(
                "the end port should be greater than the start port"
            ));
        }

        Ok(())
    }
}

impl FromStr for PortRange {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((prefix, suffix)) = s.split_once('-') {
            let start = NonZeroU16::from_str(prefix.trim())
                .map_err(|e| anyhow!("the prefix part is not a valid port number: {e}"))?;
            let end = NonZeroU16::from_str(suffix.trim())
                .map_err(|e| anyhow!("the suffix part is not a valid port number: {e}"))?;

            let range = PortRange { start, end };
            range.check()?;
            Ok(range)
        } else {
            Err(anyhow!("no delimiter found"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(value: u16) -> NonZeroU16 {
        NonZeroU16::new(value).unwrap()
    }

    #[test]
    fn normal() {
        let r = PortRange::new(port(61000), port(62000));

        let range = PortRange::from_str("61000-62000").unwrap();
        assert_eq!(r, range);

        let range = PortRange::from_str("61000 - 62000").unwrap();
        assert_eq!(r, range);
    }

    #[test]
    fn full_range_counts_every_valid_port() {
        let range = PortRange::from_str("1-65535").unwrap();
        assert_eq!(range.start(), 1);
        assert_eq!(range.end(), 65535);
        assert_eq!(range.count(), 65535);
    }

    #[test]
    fn parse_error() {
        assert!(PortRange::from_str("61000").is_err());
        assert!(PortRange::from_str("0-10000").is_err());
        assert!(PortRange::from_str("1-0").is_err());
        assert!(PortRange::from_str("1-1").is_err());
    }
}
