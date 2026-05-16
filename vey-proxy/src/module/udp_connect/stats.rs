/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use vey_daemon::stat::remote::ArcUdpConnectTaskRemoteStats;
use vey_io_ext::{LimitedRecvStats, LimitedSendStats};

use crate::auth::UserUpstreamTrafficStats;

#[derive(Clone)]
pub(crate) struct UdpConnectRemoteWrapperStats {
    all: Vec<ArcUdpConnectTaskRemoteStats>,
}

impl UdpConnectRemoteWrapperStats {
    pub(crate) fn new(
        escaper: ArcUdpConnectTaskRemoteStats,
        task: ArcUdpConnectTaskRemoteStats,
    ) -> Self {
        let mut all = Vec::with_capacity(4);
        all.push(task);
        all.push(escaper);
        UdpConnectRemoteWrapperStats { all }
    }

    pub(crate) fn push_user_io_stats(&mut self, all: Vec<Arc<UserUpstreamTrafficStats>>) {
        for s in all {
            self.all.push(s as ArcUdpConnectTaskRemoteStats);
        }
    }
}

impl LimitedRecvStats for UdpConnectRemoteWrapperStats {
    fn add_recv_bytes(&self, size: usize) {
        let size = size as u64;
        self.all.iter().for_each(|stats| stats.add_recv_bytes(size));
    }

    fn add_recv_packets(&self, n: usize) {
        self.all.iter().for_each(|stats| stats.add_recv_packets(n));
    }
}

impl LimitedSendStats for UdpConnectRemoteWrapperStats {
    fn add_send_bytes(&self, size: usize) {
        let size = size as u64;
        self.all.iter().for_each(|stats| stats.add_send_bytes(size));
    }

    fn add_send_packets(&self, n: usize) {
        self.all.iter().for_each(|stats| stats.add_send_packets(n));
    }
}
