/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use vey_daemon::stat::task::UdpConnectTaskStats;
use vey_io_ext::{LimitedRecvStats, LimitedSendStats};

use super::UdpStreamServerStats;
use crate::auth::UserTrafficStats;

trait UdpConnectTaskCltStatsWrapper {
    fn add_recv_bytes(&self, size: u64);
    #[allow(unused)]
    fn add_recv_packet(&self) {
        self.add_recv_packets(1);
    }
    fn add_recv_packets(&self, n: usize);
    fn add_send_bytes(&self, size: u64);
    #[allow(unused)]
    fn add_send_packet(&self) {
        self.add_send_packets(1);
    }
    fn add_send_packets(&self, n: usize);
}

type ArcUdpConnectTaskCltStatsWrapper = Arc<dyn UdpConnectTaskCltStatsWrapper + Send + Sync>;

impl UdpConnectTaskCltStatsWrapper for UserTrafficStats {
    fn add_recv_bytes(&self, size: u64) {
        self.io.udp_connect.add_in_bytes(size);
    }

    fn add_recv_packets(&self, n: usize) {
        self.io.udp_connect.add_in_packets(n);
    }

    fn add_send_bytes(&self, size: u64) {
        self.io.udp_connect.add_out_bytes(size);
    }

    fn add_send_packets(&self, n: usize) {
        self.io.udp_connect.add_out_packets(n);
    }
}

#[derive(Clone)]
pub(crate) struct UdpStreamTaskCltWrapperStats {
    server: Arc<UdpStreamServerStats>,
    task: Arc<UdpConnectTaskStats>,
    others: Vec<ArcUdpConnectTaskCltStatsWrapper>,
}

impl UdpStreamTaskCltWrapperStats {
    pub(crate) fn new(server: &Arc<UdpStreamServerStats>, task: &Arc<UdpConnectTaskStats>) -> Self {
        UdpStreamTaskCltWrapperStats {
            server: Arc::clone(server),
            task: Arc::clone(task),
            others: Vec::with_capacity(2),
        }
    }

    pub(crate) fn push_user_io_stats(&mut self, all: Vec<Arc<UserTrafficStats>>) {
        for s in all {
            self.others.push(s);
        }
    }
}

impl LimitedRecvStats for UdpStreamTaskCltWrapperStats {
    fn add_recv_bytes(&self, size: usize) {
        let size = size as u64;
        self.server.udp.add_in_bytes(size);
        self.task.clt.recv.add_bytes(size);
        self.others.iter().for_each(|s| s.add_recv_bytes(size));
    }

    fn add_recv_packets(&self, n: usize) {
        self.server.udp.add_in_packets(n);
        self.task.clt.recv.add_packets(n);
        self.others.iter().for_each(|s| s.add_recv_packets(n));
    }
}

impl LimitedSendStats for UdpStreamTaskCltWrapperStats {
    fn add_send_bytes(&self, size: usize) {
        let size = size as u64;
        self.server.udp.add_out_bytes(size);
        self.task.clt.send.add_bytes(size);
        self.others.iter().for_each(|s| s.add_send_bytes(size));
    }

    fn add_send_packets(&self, n: usize) {
        self.server.udp.add_out_packets(n);
        self.task.clt.send.add_packets(n);
        self.others.iter().for_each(|s| s.add_send_packets(n));
    }
}
