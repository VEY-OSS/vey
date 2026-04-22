/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod var_int;
pub use var_int::VarInt;

mod frame;
pub use frame::{AckFrame, AckRange, CryptoFrame, EcnCounts, FrameConsume, FrameParseError};

mod packet;
pub use packet::{InitialPacket, PacketParseError};

mod message;
pub use message::HandshakeCoalescer;

#[cfg(test)]
mod tests;
