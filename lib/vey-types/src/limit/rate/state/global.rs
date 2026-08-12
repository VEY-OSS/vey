/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::RateLimitState;

#[derive(Default)]
pub struct GlobalRateLimitState(AtomicU64);

impl GlobalRateLimitState {
    #[cfg(test)]
    pub(crate) fn target_t(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

impl RateLimitState for GlobalRateLimitState {
    fn fetch_and_update<F>(&self, update: F) -> Result<(), Duration>
    where
        F: Fn(u64) -> Result<u64, Duration>,
    {
        let mut err = None;
        match self
            .0
            .try_update(Ordering::Acquire, Ordering::Relaxed, |cur| {
                match update(cur) {
                    Ok(next) => Some(next),
                    Err(d) => {
                        err = Some(d);
                        None
                    }
                }
            }) {
            Ok(_) => Ok(()),
            Err(_) => Err(err.expect("try_update failed only after update returned Err")),
        }
    }
}

impl RateLimitState for Arc<GlobalRateLimitState> {
    fn fetch_and_update<F>(&self, update: F) -> Result<(), Duration>
    where
        F: Fn(u64) -> Result<u64, Duration>,
    {
        self.as_ref().fetch_and_update(update)
    }
}
