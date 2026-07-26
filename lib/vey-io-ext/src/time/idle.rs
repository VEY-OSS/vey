/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::time::Duration;

use tokio::time::{Instant, Interval, MissedTickBehavior};

pub struct IdleWheel {
    interval: Duration,
}

impl IdleWheel {
    pub fn spawn(interval: Duration) -> Arc<IdleWheel> {
        Arc::new(IdleWheel { interval })
    }

    pub fn register(&self) -> IdleInterval {
        let mut x_idle = tokio::time::interval_at(Instant::now() + self.interval, self.interval);
        x_idle.set_missed_tick_behavior(MissedTickBehavior::Delay);
        IdleInterval { interval: x_idle }
    }
}

pub struct IdleInterval {
    interval: Interval,
}

impl IdleInterval {
    pub async fn tick(&mut self) -> usize {
        self.interval.tick().await;
        1
    }

    pub fn period(&self) -> Duration {
        self.interval.period()
    }
}

#[derive(Clone, Copy, Debug)]
pub enum IdleForceQuitReason {
    UserBlocked,
    ServerQuit,
}

pub trait IdleCheck {
    fn interval_timer(&self) -> IdleInterval;
    fn check_quit(&self, idle_count: usize) -> bool;
    fn check_force_quit(&self) -> Option<IdleForceQuitReason>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn idle_wheel_tick_advances() {
        let wheel = IdleWheel::spawn(Duration::from_millis(5));
        let mut interval = wheel.register();

        assert_eq!(interval.period(), Duration::from_millis(5));

        let tick = tokio::time::timeout(Duration::from_millis(50), interval.tick()).await;
        assert!(tick.is_ok());
        assert_eq!(tick.unwrap(), 1);
    }
}
