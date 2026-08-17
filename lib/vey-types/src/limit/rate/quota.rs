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
    /// Construct a quota of `cells` events per `period`.
    ///
    /// Burst is `max(1, ceil(cells allowed in 1ms))` so the limiter stays close
    /// to the steady rate without being clipped by millisecond timer granularity.
    /// Use [`allow_burst`](Self::allow_burst) when a larger burst is needed.
    pub fn new(period: Duration, cells: NonZeroU32) -> anyhow::Result<Self> {
        let period_nanos = period.as_nanos_u64();
        let cell_count = u64::from(cells.get());
        let replenish_nanos = period_nanos / cell_count;
        let replenish_nanos = NonZeroU64::new(replenish_nanos)
            .ok_or_else(|| anyhow!("too large cell count {cells} within {period:?} period"))?;
        let cells_in_1ms = cell_count
            .saturating_mul(Duration::from_millis(1).as_nanos_u64())
            .div_ceil(period_nanos);
        let max_burst =
            NonZeroU32::new(u32::try_from(cells_in_1ms).unwrap_or(u32::MAX).max(1)).unwrap();
        Ok(RateLimitQuota {
            max_burst,
            replenish_nanos,
        })
    }

    pub fn per_second(cells: NonZeroU32) -> anyhow::Result<Self> {
        Self::new(Duration::from_secs(1), cells)
    }

    pub fn per_minute(cells: NonZeroU32) -> anyhow::Result<Self> {
        Self::new(Duration::from_secs(60), cells)
    }

    pub fn per_hour(cells: NonZeroU32) -> anyhow::Result<Self> {
        Self::new(Duration::from_secs(3600), cells)
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

        let v = RateLimitQuota::with_period(Duration::from_secs(1)).unwrap();
        assert_eq!(RateLimitQuota::from_str("60/m").unwrap(), v);
        assert_eq!(RateLimitQuota::from_str("3600/h").unwrap(), v);
    }

    #[test]
    fn t_new_flattens_burst() {
        let cells = NonZeroU32::new(30).unwrap();
        let q = RateLimitQuota::new(Duration::from_secs(1), cells).unwrap();
        assert_eq!(q.max_burst, NonZeroU32::MIN);
        assert_eq!(
            q.replenish_nanos,
            NonZeroU64::new(1_000_000_000 / 30).unwrap()
        );

        let q = RateLimitQuota::per_second(NonZeroU32::new(1_000).unwrap()).unwrap();
        assert_eq!(q.max_burst, NonZeroU32::MIN);

        let q = RateLimitQuota::per_second(NonZeroU32::new(1_200).unwrap()).unwrap();
        assert_eq!(q.max_burst, NonZeroU32::new(2).unwrap());

        let q = RateLimitQuota::per_second(NonZeroU32::new(2_000).unwrap()).unwrap();
        assert_eq!(q.max_burst, NonZeroU32::new(2).unwrap());

        let q = RateLimitQuota::per_second(NonZeroU32::new(10_000).unwrap()).unwrap();
        assert_eq!(q.max_burst, NonZeroU32::new(10).unwrap());

        let mut q = RateLimitQuota::per_second(cells).unwrap();
        q.allow_burst(cells);
        assert_eq!(q.max_burst, cells);
    }
}
