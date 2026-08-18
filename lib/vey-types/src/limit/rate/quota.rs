/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::num::{NonZeroU32, NonZeroU64};
use std::str::FromStr;
use std::time::Duration;

use anyhow::anyhow;

use vey_std_ext::time::DurationExt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RateLimitQuota {
    pub(super) max_burst: NonZeroU32,
    pub(super) replenish_nanos: NonZeroU64,
}

impl RateLimitQuota {
    pub fn new(period: Duration, max_burst: NonZeroU32) -> anyhow::Result<Self> {
        let replenish_nanos = period.as_nanos_u64() / (max_burst.get() as u64);
        let replenish_nanos = NonZeroU64::new(replenish_nanos).ok_or_else(|| {
            anyhow!("too large max burst value {max_burst} within {period:?} period")
        })?;
        Ok(RateLimitQuota {
            max_burst,
            replenish_nanos,
        })
    }

    pub fn per_second(max_burst: NonZeroU32) -> anyhow::Result<Self> {
        Self::new(Duration::from_secs(1), max_burst)
    }

    pub fn per_minute(max_burst: NonZeroU32) -> anyhow::Result<Self> {
        Self::new(Duration::from_secs(60), max_burst)
    }

    pub fn per_hour(max_burst: NonZeroU32) -> anyhow::Result<Self> {
        Self::new(Duration::from_secs(3600), max_burst)
    }

    /// Like [`new`](Self::new), but burst is paced to about 1ms of traffic.
    pub fn paced_new(period: Duration, cells: NonZeroU32) -> anyhow::Result<Self> {
        let mut quota = Self::new(period, cells)?;
        quota.pace();
        Ok(quota)
    }

    pub fn paced_per_second(cells: NonZeroU32) -> anyhow::Result<Self> {
        Self::paced_new(Duration::from_secs(1), cells)
    }

    pub fn paced_per_minute(cells: NonZeroU32) -> anyhow::Result<Self> {
        Self::paced_new(Duration::from_secs(60), cells)
    }

    pub fn paced_per_hour(cells: NonZeroU32) -> anyhow::Result<Self> {
        Self::paced_new(Duration::from_secs(3600), cells)
    }

    pub fn with_period(period: Duration) -> Option<Self> {
        let replenish_nanos = NonZeroU64::new(period.as_nanos_u64())?;
        Some(RateLimitQuota {
            max_burst: NonZeroU32::MIN,
            replenish_nanos,
        })
    }

    pub fn allow_burst(&mut self, max_burst: NonZeroU32) {
        self.max_burst = max_burst;
    }

    /// Shrink burst to about 1ms of traffic for paced request generation.
    ///
    /// Rate-limit checks reject immediately, so they keep the constructed
    /// burst (usually equal to the rate). Load generators wait instead, and
    /// a full-period burst dumps a spike; this keeps the cell interval and
    /// sets burst to `ceil(1ms / cell)` so a ~1ms timer does not clip rates
    /// such as `1200/s` down to `1000/s`.
    pub fn pace(&mut self) {
        const TICK_NS: u64 = 1_000_000;
        let cell = self.replenish_nanos.get();
        let burst = TICK_NS.div_ceil(cell).min(u64::from(u32::MAX));
        self.max_burst = NonZeroU32::new(burst as u32).unwrap_or(NonZeroU32::MIN);
    }
}

impl FromStr for RateLimitQuota {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once('/') {
            Some((v1, v2)) => {
                let u = NonZeroU32::from_str(v1.trim())
                    .map_err(|_| anyhow!("invalid non-zero u32 string as the first part"))?;
                match v2 {
                    "s" => RateLimitQuota::per_second(u),
                    "m" => RateLimitQuota::per_minute(u),
                    "h" => RateLimitQuota::per_hour(u),
                    _ => Err(anyhow!("invalid unit in second part")),
                }
            }
            None => {
                let u = NonZeroU32::from_str(s)
                    .map_err(|e| anyhow!("invalid non-zero u32 string: {e}"))?;
                RateLimitQuota::per_second(u)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nz(n: u32) -> NonZeroU32 {
        NonZeroU32::new(n).unwrap()
    }

    #[test]
    fn t_from_str() {
        assert_eq!(
            RateLimitQuota::from_str("30").unwrap(),
            RateLimitQuota::per_second(NonZeroU32::new(30).unwrap()).unwrap()
        );
        assert_eq!(
            RateLimitQuota::from_str("30/s").unwrap(),
            RateLimitQuota::per_second(NonZeroU32::new(30).unwrap()).unwrap()
        );

        let mut v = RateLimitQuota::with_period(Duration::from_secs(1)).unwrap();
        v.allow_burst(NonZeroU32::new(60).unwrap());
        assert_eq!(RateLimitQuota::from_str("60/m").unwrap(), v);

        v.allow_burst(NonZeroU32::new(3600).unwrap());
        assert_eq!(RateLimitQuota::from_str("3600/h").unwrap(), v);
    }

    #[test]
    fn t_new_keeps_rate_as_burst() {
        let q = RateLimitQuota::per_second(nz(500)).unwrap();
        assert_eq!(q.max_burst, nz(500));
        assert_eq!(
            q.replenish_nanos,
            NonZeroU64::new(1_000_000_000 / 500).unwrap()
        );
    }

    #[test]
    fn t_paced_keeps_cell_and_ceils_tick_burst() {
        let q = RateLimitQuota::paced_per_second(nz(10)).unwrap();
        assert_eq!(q.replenish_nanos.get(), 100_000_000);
        assert_eq!(q.max_burst, nz(1));

        let q = RateLimitQuota::paced_per_second(nz(1200)).unwrap();
        assert_eq!(q.replenish_nanos.get(), 1_000_000_000 / 1200);
        assert_eq!(q.max_burst, nz(2));

        let q = RateLimitQuota::paced_per_second(nz(10_000)).unwrap();
        assert_eq!(q.replenish_nanos.get(), 100_000);
        assert_eq!(q.max_burst, nz(10));
    }
}
