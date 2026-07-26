/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 */

use std::time::Duration;

const NANOS_PER_MILLI: u32 = 1_000_000;

pub trait DurationExt {
    fn as_millis_f64(&self) -> f64;

    fn as_nanos_u64(&self) -> u64;
}

impl DurationExt for Duration {
    fn as_millis_f64(&self) -> f64 {
        (self.as_secs() * 1000) as f64 + (self.subsec_nanos() as f64 / NANOS_PER_MILLI as f64)
    }

    fn as_nanos_u64(&self) -> u64 {
        u64::try_from(self.as_nanos()).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millis_f64() {
        assert_eq!(DurationExt::as_millis_f64(&Duration::from_millis(1500)), 1500.0);
        assert_eq!(DurationExt::as_millis_f64(&Duration::from_secs(2)), 2000.0);
        assert!(
            (DurationExt::as_millis_f64(&Duration::from_nanos(1_500_000)) - 1.5).abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn nanos_u64() {
        assert_eq!(Duration::from_nanos(100).as_nanos_u64(), 100);
        assert_eq!(Duration::MAX.as_nanos_u64(), u64::MAX);
    }
}
