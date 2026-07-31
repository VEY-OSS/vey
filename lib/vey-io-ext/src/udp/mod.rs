/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod stats;
pub use stats::{ArcLimitedRecvStats, ArcLimitedSendStats, LimitedRecvStats, LimitedSendStats};

mod ext;
pub use ext::*;

mod recv;
mod send;

pub use recv::{AsyncUdpRecv, LimitedUdpRecv};
pub use send::{AsyncUdpSend, LimitedUdpSend};

mod relay;
pub use relay::{
    UdpRelayClientError, UdpRelayClientRecv, UdpRelayClientSend, UdpRelayPacket,
    UdpRelayPacketMeta, UdpRelayRemoteError, UdpRelayRemoteRecv, UdpRelayRemoteSend,
};
pub use relay::{UdpRelayClientToRemote, UdpRelayError, UdpRelayRemoteToClient};

mod stream_copy;
pub use stream_copy::{
    AsUdpPayload, LimitedUdpCopyClientRecv, LimitedUdpCopyClientSend, LimitedUdpCopyRemoteRecv,
    LimitedUdpCopyRemoteSend, UdpCopyClientError, UdpCopyClientRecv, UdpCopyClientSend,
    UdpCopyClientToRemote, UdpCopyError, UdpCopyPacket, UdpCopyPacketMeta, UdpCopyRemoteError,
    UdpCopyRemoteRecv, UdpCopyRemoteSend, UdpCopyRemoteToClient,
};

mod stream_move;
pub use stream_move::{
    LimitedUdpMoveRecv, LimitedUdpMoveSend, UdpMoveError, UdpMoveRecv, UdpMoveRemoteReceiver,
    UdpMoveRemoteSender, UdpMoveSend, UdpMoveTransfer,
};

mod split;
pub use split::{
    RecvHalf as UdpRecvHalf, ReuniteError as UdpReuniteError, SendHalf as UdpSendHalf,
    split as split_udp,
};

const DEFAULT_UDP_PACKET_SIZE: u16 = 4096; // at least for DNS with extension
const DEFAULT_UDP_RELAY_YIELD_COUNT: usize = 1024;
const DEFAULT_UDP_RELAY_BATCH_COUNT: usize = 8;
const DEFAULT_UDP_UNDERLYING_BUFFER_SIZE: usize = 16384;
const MINIMUM_UDP_PACKET_SIZE: u16 = 512;
const MAXIMUM_UDP_PACKET_SIZE: u16 = 16 * 1024;
const MINIMUM_UDP_RELAY_YIELD_COUNT: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LimitedUdpRelayConfig {
    packet_size: u16,
    yield_count: usize,
    batch_count: usize,
    underlying_buffer_size: usize,
}

impl Default for LimitedUdpRelayConfig {
    fn default() -> Self {
        LimitedUdpRelayConfig {
            packet_size: DEFAULT_UDP_PACKET_SIZE,
            yield_count: DEFAULT_UDP_RELAY_YIELD_COUNT,
            batch_count: DEFAULT_UDP_RELAY_BATCH_COUNT,
            underlying_buffer_size: DEFAULT_UDP_UNDERLYING_BUFFER_SIZE,
        }
    }
}

impl LimitedUdpRelayConfig {
    pub fn set_packet_size(&mut self, packet_size: u16) {
        self.packet_size = packet_size.clamp(MINIMUM_UDP_PACKET_SIZE, MAXIMUM_UDP_PACKET_SIZE);
    }

    #[inline]
    pub fn packet_size(&self) -> u16 {
        self.packet_size
    }

    pub fn set_yield_count(&mut self, yield_count: usize) {
        self.yield_count = yield_count.max(MINIMUM_UDP_RELAY_YIELD_COUNT);
    }

    pub fn set_batch_count(&mut self, batch_count: usize) {
        self.batch_count = batch_count;
    }

    pub fn set_underlying_buffer_size(&mut self, underlying_buffer_size: usize) {
        self.underlying_buffer_size = underlying_buffer_size;
    }

    pub fn underlying_buffer_size(&self) -> usize {
        self.underlying_buffer_size
            .max(self.packet_size as usize * self.batch_count.min(DEFAULT_UDP_RELAY_BATCH_COUNT))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let cfg = LimitedUdpRelayConfig::default();
        assert_eq!(cfg.packet_size(), DEFAULT_UDP_PACKET_SIZE);
        assert_eq!(
            cfg.underlying_buffer_size(),
            (DEFAULT_UDP_PACKET_SIZE as usize * DEFAULT_UDP_RELAY_BATCH_COUNT)
                .max(DEFAULT_UDP_UNDERLYING_BUFFER_SIZE)
        );
    }

    #[test]
    fn set_packet_size_clamps_to_valid_range() {
        let mut cfg = LimitedUdpRelayConfig::default();
        cfg.set_packet_size(1);
        assert_eq!(cfg.packet_size(), MINIMUM_UDP_PACKET_SIZE);
        cfg.set_packet_size(u16::MAX);
        assert_eq!(cfg.packet_size(), MAXIMUM_UDP_PACKET_SIZE);
        cfg.set_packet_size(1500);
        assert_eq!(cfg.packet_size(), 1500);
    }

    #[test]
    fn set_yield_count_enforces_minimum() {
        let mut cfg = LimitedUdpRelayConfig::default();
        cfg.set_yield_count(1);
        assert_eq!(cfg.yield_count, MINIMUM_UDP_RELAY_YIELD_COUNT);
        cfg.set_yield_count(4096);
        assert_eq!(cfg.yield_count, 4096);
    }

    #[test]
    fn underlying_buffer_size_grows_with_packet_and_batch() {
        let mut cfg = LimitedUdpRelayConfig::default();
        cfg.set_underlying_buffer_size(0);
        cfg.set_packet_size(1024);
        cfg.set_batch_count(4);
        assert_eq!(cfg.underlying_buffer_size(), 1024 * 4);
    }

    #[test]
    fn underlying_buffer_size_caps_the_batch_used_as_floor() {
        let mut cfg = LimitedUdpRelayConfig::default();
        cfg.set_underlying_buffer_size(0);
        cfg.set_packet_size(1024);
        cfg.set_batch_count(1024);
        assert_eq!(
            cfg.underlying_buffer_size(),
            1024 * DEFAULT_UDP_RELAY_BATCH_COUNT
        );
    }

    #[test]
    fn underlying_buffer_size_keeps_the_larger_explicit_value() {
        let mut cfg = LimitedUdpRelayConfig::default();
        cfg.set_packet_size(MINIMUM_UDP_PACKET_SIZE);
        cfg.set_batch_count(1);
        cfg.set_underlying_buffer_size(1 << 20);
        assert_eq!(cfg.underlying_buffer_size(), 1 << 20);
    }

    #[test]
    fn underlying_buffer_size_with_zero_batch_falls_back_to_explicit_value() {
        let mut cfg = LimitedUdpRelayConfig::default();
        cfg.set_batch_count(0);
        cfg.set_underlying_buffer_size(64);
        assert_eq!(cfg.underlying_buffer_size(), 64);
    }

    #[test]
    fn set_packet_size_accepts_the_exact_bounds() {
        let mut cfg = LimitedUdpRelayConfig::default();
        cfg.set_packet_size(MINIMUM_UDP_PACKET_SIZE);
        assert_eq!(cfg.packet_size(), MINIMUM_UDP_PACKET_SIZE);
        cfg.set_packet_size(MAXIMUM_UDP_PACKET_SIZE);
        assert_eq!(cfg.packet_size(), MAXIMUM_UDP_PACKET_SIZE);
    }

    #[test]
    fn set_yield_count_accepts_the_exact_minimum() {
        let mut cfg = LimitedUdpRelayConfig::default();
        cfg.set_yield_count(MINIMUM_UDP_RELAY_YIELD_COUNT);
        assert_eq!(cfg.yield_count, MINIMUM_UDP_RELAY_YIELD_COUNT);
        cfg.set_yield_count(0);
        assert_eq!(cfg.yield_count, MINIMUM_UDP_RELAY_YIELD_COUNT);
    }

    #[test]
    fn config_copy_keeps_the_source_unchanged() {
        let mut cfg = LimitedUdpRelayConfig::default();
        cfg.set_packet_size(1200);
        let copied = cfg;
        cfg.set_packet_size(1300);
        assert_eq!(copied.packet_size(), 1200);
        assert_ne!(copied, cfg);
    }
}
