/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

pub trait UdpConnectTaskRemoteStats {
    fn add_recv_bytes(&self, size: u64);
    fn add_recv_packet(&self) {
        self.add_recv_packets(1);
    }
    fn add_recv_packets(&self, n: usize);
    fn add_send_bytes(&self, size: u64);
    fn add_send_packet(&self) {
        self.add_send_packets(1);
    }
    fn add_send_packets(&self, n: usize);
}

pub type ArcUdpConnectTaskRemoteStats = Arc<dyn UdpConnectTaskRemoteStats + Send + Sync>;
