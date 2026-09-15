/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use h2::{Ping, PingPong};
use tokio::sync::oneshot;

pub(super) fn spawn_ping(mut ping: PingPong, interval: Duration, mut quit: oneshot::Receiver<()>) {
    if interval.is_zero() {
        return;
    }
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            tokio::select! {
                _ = &mut quit => break,
                _ = ticker.tick() => {
                    if ping.ping(Ping::opaque()).await.is_err() {
                        break;
                    }
                }
            }
        }
    });
}
