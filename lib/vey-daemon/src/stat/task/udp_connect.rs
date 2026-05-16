/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::cell::UnsafeCell;

use crate::stat::remote::UdpConnectTaskRemoteStats;

#[derive(Default)]
pub struct UdpConnectHalfConnectionStats {
    bytes: UnsafeCell<u64>,
    packets: UnsafeCell<u64>,
}

unsafe impl Sync for UdpConnectHalfConnectionStats {}

impl UdpConnectHalfConnectionStats {
    pub fn get_bytes(&self) -> u64 {
        let r = unsafe { &*self.bytes.get() };
        *r
    }

    pub fn get_packets(&self) -> u64 {
        let r = unsafe { &*self.packets.get() };
        *r
    }

    pub fn add_bytes(&self, size: u64) {
        let r = unsafe { &mut *self.bytes.get() };
        *r += size;
    }

    pub fn add_packet(&self) {
        self.add_packets(1);
    }

    pub fn add_packets(&self, n: usize) {
        let r = unsafe { &mut *self.packets.get() };
        *r += n as u64;
    }
}

#[derive(Default)]
pub struct UdpConnectConnectionStats {
    pub recv: UdpConnectHalfConnectionStats,
    pub send: UdpConnectHalfConnectionStats,
}

#[derive(Default)]
pub struct UdpConnectTaskStats {
    pub clt: UdpConnectConnectionStats,
    pub ups: UdpConnectConnectionStats,
}

impl UdpConnectTaskRemoteStats for UdpConnectTaskStats {
    fn add_recv_bytes(&self, size: u64) {
        self.ups.recv.add_bytes(size);
    }

    fn add_recv_packets(&self, n: usize) {
        self.ups.recv.add_packets(n);
    }

    fn add_send_bytes(&self, size: u64) {
        self.ups.send.add_bytes(size);
    }

    fn add_send_packets(&self, n: usize) {
        self.ups.send.add_packets(n);
    }
}
