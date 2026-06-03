/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use vey_daemon::stat::task::UdpConnectTaskStats;
use vey_io_ext::{LimitedReaderStats, LimitedRecvStats, LimitedSendStats, LimitedWriterStats};

use crate::auth::UserTrafficStats;

use super::HttpProxyServerStats;

#[derive(Clone)]
pub(super) struct MasqueUdpTaskServerCltWrapperStats(Arc<HttpProxyServerStats>);

impl MasqueUdpTaskServerCltWrapperStats {
    pub(super) fn new(inner: Arc<HttpProxyServerStats>) -> Self {
        MasqueUdpTaskServerCltWrapperStats(inner)
    }
}

impl LimitedReaderStats for MasqueUdpTaskServerCltWrapperStats {
    fn add_read_bytes(&self, size: usize) {
        self.0.io_masque_udp.add_in_bytes(size as u64);
    }
}

impl LimitedWriterStats for MasqueUdpTaskServerCltWrapperStats {
    fn add_write_bytes(&self, size: usize) {
        self.0.io_masque_udp.add_out_bytes(size as u64);
    }
}

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
        self.io.http_masque_udp.add_in_bytes(size);
    }

    fn add_recv_packets(&self, n: usize) {
        self.io.http_masque_udp.add_in_packets(n);
    }

    fn add_send_bytes(&self, size: u64) {
        self.io.http_masque_udp.add_out_bytes(size);
    }

    fn add_send_packets(&self, n: usize) {
        self.io.http_masque_udp.add_out_packets(n);
    }
}

#[derive(Clone)]
pub(crate) struct MasqueUdpTaskCltWrapperStats {
    task: Arc<UdpConnectTaskStats>,
    others: Vec<ArcUdpConnectTaskCltStatsWrapper>,
}

impl MasqueUdpTaskCltWrapperStats {
    pub(crate) fn new(task: &Arc<UdpConnectTaskStats>) -> Self {
        MasqueUdpTaskCltWrapperStats {
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

impl LimitedRecvStats for MasqueUdpTaskCltWrapperStats {
    fn add_recv_bytes(&self, size: usize) {
        let size = size as u64;
        self.task.clt.recv.add_bytes(size);
        self.others.iter().for_each(|s| s.add_recv_bytes(size));
    }

    fn add_recv_packets(&self, n: usize) {
        self.task.clt.recv.add_packets(n);
        self.others.iter().for_each(|s| s.add_recv_packets(n));
    }
}

impl LimitedSendStats for MasqueUdpTaskCltWrapperStats {
    fn add_send_bytes(&self, size: usize) {
        let size = size as u64;
        self.task.clt.send.add_bytes(size);
        self.others.iter().for_each(|s| s.add_send_bytes(size));
    }

    fn add_send_packets(&self, n: usize) {
        self.task.clt.send.add_packets(n);
        self.others.iter().for_each(|s| s.add_send_packets(n));
    }
}
