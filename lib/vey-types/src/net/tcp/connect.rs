/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TcpConnectConfig {
    max_tries: usize,
    each_timeout: Duration,
}

impl Default for TcpConnectConfig {
    fn default() -> Self {
        TcpConnectConfig {
            max_tries: 3,
            each_timeout: Duration::from_secs(30),
        }
    }
}

impl TcpConnectConfig {
    pub fn set_max_retry(&mut self, max_retry: usize) {
        self.max_tries = max_retry.saturating_add(1);
    }

    #[inline]
    pub fn max_tries(&self) -> usize {
        self.max_tries
    }

    pub fn set_each_timeout(&mut self, each_timeout: Duration) {
        if !each_timeout.is_zero() {
            self.each_timeout = each_timeout;
        }
    }

    #[inline]
    pub fn each_timeout(&self) -> Duration {
        self.each_timeout
    }

    pub fn limit_to(&mut self, other: &Self) {
        self.max_tries = self.max_tries.min(other.max_tries);
        if !other.each_timeout.is_zero() {
            self.each_timeout = self.each_timeout.min(other.each_timeout);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HappyEyeballsConfig {
    first_resolution_delay: Duration,
    second_resolution_timeout: Duration,
    first_address_family_count: usize,
    connection_attempt_delay: Duration,
}

impl Default for HappyEyeballsConfig {
    fn default() -> Self {
        HappyEyeballsConfig {
            first_resolution_delay: Duration::from_millis(50),
            second_resolution_timeout: Duration::from_secs(2),
            first_address_family_count: 1,
            connection_attempt_delay: Duration::from_millis(250),
        }
    }
}

impl HappyEyeballsConfig {
    #[inline]
    pub fn resolution_delay(&self) -> Duration {
        self.first_resolution_delay
    }

    pub fn set_resolution_delay(&mut self, delay: Duration) {
        self.first_resolution_delay = delay;
    }

    #[inline]
    pub fn second_resolution_timeout(&self) -> Duration {
        self.second_resolution_timeout
    }

    pub fn set_second_resolution_timeout(&mut self, timeout: Duration) {
        self.second_resolution_timeout = timeout;
    }

    #[inline]
    pub fn first_address_family_count(&self) -> usize {
        self.first_address_family_count
    }

    pub fn set_first_address_family_count(&mut self, count: usize) {
        self.first_address_family_count = count;
    }

    #[inline]
    pub fn connection_attempt_delay(&self) -> Duration {
        self.connection_attempt_delay
    }

    pub fn set_connection_attempt_delay(&mut self, delay: Duration) {
        self.connection_attempt_delay =
            delay.clamp(Duration::from_millis(100), Duration::from_secs(2))
    }

    /// Interleave `new` into `ips`, then move items for which `defer` is true to the front.
    ///
    /// `pop()` tries the tail first. The next `first_address_family_count - tried`
    /// addresses of `ips` stay next, then addresses from `new` alternate with what
    /// remains. Deferred addresses are attempted last. Relative order inside each
    /// group stays. A deferred address does not change which healthy address is next.
    pub fn merge_list<T, F>(&self, tried: usize, ips: &mut Vec<T>, new: Vec<T>, defer: F)
    where
        F: Fn(&T) -> bool,
    {
        // After reversing, index 0 is the next address `pop()` would return.
        ips.reverse();
        let gap = self
            .first_address_family_count
            .saturating_sub(tried)
            .min(ips.len());
        ips.reserve(new.len());
        let mut insert_at = gap;
        for ip in new {
            if insert_at < ips.len() {
                ips.insert(insert_at, ip);
                insert_at += 2;
            } else {
                ips.push(ip);
                insert_at += 1;
            }
        }
        if ips.len() >= 2 {
            // Move deferred addresses to the end, keeping the order inside each group.
            let mut index = 0;
            let mut remaining = ips.len();
            while index < remaining {
                if defer(&ips[index]) {
                    let ip = ips.remove(index);
                    ips.push(ip);
                    remaining -= 1;
                } else {
                    index += 1;
                }
            }
        }
        ips.reverse();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_max_retry_saturates() {
        let mut config = TcpConnectConfig::default();
        config.set_max_retry(usize::MAX);
        assert_eq!(config.max_tries(), usize::MAX);
    }

    #[test]
    fn set_each_timeout_rejects_zero() {
        let mut config = TcpConnectConfig::default();
        let default_timeout = config.each_timeout();
        config.set_each_timeout(Duration::ZERO);
        assert_eq!(config.each_timeout(), default_timeout);

        config.set_each_timeout(Duration::from_secs(5));
        assert_eq!(config.each_timeout(), Duration::from_secs(5));
    }

    #[test]
    fn limit_to_ignores_zero_timeout() {
        let mut config = TcpConnectConfig::default();
        config.set_each_timeout(Duration::from_secs(10));

        let other = TcpConnectConfig {
            each_timeout: Duration::ZERO,
            ..Default::default()
        };
        // Simulate a zero timeout that should not clamp us down to immediate failure.
        config.limit_to(&other);
        assert_eq!(config.each_timeout(), Duration::from_secs(10));
    }

    #[test]
    fn merge_list_tries_deferred_addresses_last() {
        let mut ips = vec![1, 2];
        HappyEyeballsConfig::default().merge_list(0, &mut ips, vec![3, 4], |n| *n == 3);
        // pop order was 2, 1. After one address of the first family, alternate 3 then 1 then 4.
        // 3 is deferred, so the attempt order is 2, 1, 4, 3.
        assert_eq!(ips, vec![3, 4, 1, 2]);
    }

    #[test]
    fn merge_list_interleaves_the_other_family_at_the_pop_end() {
        let mut ips = vec![2, 1];
        HappyEyeballsConfig::default().merge_list(0, &mut ips, vec![3, 4], |_| false);
        assert_eq!(ips, vec![4, 2, 3, 1]);
        assert_eq!(ips.pop(), Some(1));
        assert_eq!(ips.pop(), Some(3));
        assert_eq!(ips.pop(), Some(2));
        assert_eq!(ips.pop(), Some(4));
    }

    #[test]
    fn merge_list_deferred_address_does_not_change_the_next_healthy_one() {
        let mut ips = vec![1];
        HappyEyeballsConfig::default().merge_list(1, &mut ips, vec![2, 3], |n| *n == 3);
        assert_eq!(ips.pop(), Some(2));
        assert_eq!(ips.pop(), Some(1));
        assert_eq!(ips.pop(), Some(3));
    }
}
