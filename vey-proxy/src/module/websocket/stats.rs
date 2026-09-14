/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use vey_daemon::stat::remote::TcpConnectionTaskRemoteStats;
use vey_daemon::stat::task::TcpStreamConnectionStats;

#[derive(Default)]
pub(crate) struct WebSocketTaskStats {
    pub(crate) clt: TcpStreamConnectionStats,
    pub(crate) ups: TcpStreamConnectionStats,
}

impl TcpConnectionTaskRemoteStats for WebSocketTaskStats {
    fn add_read_bytes(&self, size: u64) {
        self.ups.read.add_bytes(size);
    }
    fn add_write_bytes(&self, size: u64) {
        self.ups.write.add_bytes(size);
    }
}
