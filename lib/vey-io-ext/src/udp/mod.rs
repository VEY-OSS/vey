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
