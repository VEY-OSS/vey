/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use crate::NilLimitedStats;

pub trait LimitedRecvStats {
    fn add_recv_bytes(&self, size: usize);
    fn add_recv_packet(&self) {
        self.add_recv_packets(1);
    }
    fn add_recv_packets(&self, n: usize);
}
pub type ArcLimitedRecvStats = Arc<dyn LimitedRecvStats + Send + Sync>;

impl LimitedRecvStats for NilLimitedStats {
    fn add_recv_bytes(&self, _size: usize) {}

    fn add_recv_packet(&self) {}

    fn add_recv_packets(&self, _n: usize) {}
}

pub trait LimitedSendStats {
    fn add_send_bytes(&self, size: usize);
    fn add_send_packet(&self) {
        self.add_send_packets(1);
    }
    fn add_send_packets(&self, n: usize);
}
pub type ArcLimitedSendStats = Arc<dyn LimitedSendStats + Send + Sync>;

impl LimitedSendStats for NilLimitedStats {
    fn add_send_bytes(&self, _size: usize) {}

    fn add_send_packet(&self) {}

    fn add_send_packets(&self, _n: usize) {}
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[derive(Default)]
    struct Counters {
        packets: AtomicUsize,
        calls: AtomicUsize,
        bytes: AtomicUsize,
    }

    impl LimitedRecvStats for Counters {
        fn add_recv_bytes(&self, size: usize) {
            self.bytes.fetch_add(size, Ordering::Relaxed);
        }

        fn add_recv_packets(&self, n: usize) {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.packets.fetch_add(n, Ordering::Relaxed);
        }
    }

    impl LimitedSendStats for Counters {
        fn add_send_bytes(&self, size: usize) {
            self.bytes.fetch_add(size, Ordering::Relaxed);
        }

        fn add_send_packets(&self, n: usize) {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.packets.fetch_add(n, Ordering::Relaxed);
        }
    }

    #[test]
    fn add_recv_packet_forwards_a_single_packet() {
        let counters = Arc::new(Counters::default());
        let stats: ArcLimitedRecvStats = counters.clone();
        stats.add_recv_packet();
        stats.add_recv_packets(4);
        stats.add_recv_bytes(100);

        assert_eq!(counters.calls.load(Ordering::Relaxed), 2);
        assert_eq!(counters.packets.load(Ordering::Relaxed), 5);
        assert_eq!(counters.bytes.load(Ordering::Relaxed), 100);
    }

    #[test]
    fn add_send_packet_forwards_a_single_packet() {
        let counters = Arc::new(Counters::default());
        let stats: ArcLimitedSendStats = counters.clone();
        stats.add_send_packet();
        stats.add_send_packets(3);
        stats.add_send_bytes(64);

        assert_eq!(counters.calls.load(Ordering::Relaxed), 2);
        assert_eq!(counters.packets.load(Ordering::Relaxed), 4);
        assert_eq!(counters.bytes.load(Ordering::Relaxed), 64);
    }
}
