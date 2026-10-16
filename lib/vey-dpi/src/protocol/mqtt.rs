/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use vey_codec::leb128::{Leb128, Leb128DecodeError};

use super::{MaybeProtocol, Protocol, ProtocolInspectError, ProtocolInspectState};

impl ProtocolInspectState {
    pub(crate) fn check_mqtt_client_connect_request(
        &mut self,
        data: &[u8],
    ) -> Result<Option<Protocol>, ProtocolInspectError> {
        // at least FixedHeader, Protocol Name, Protocol Level, Connect Flags, and Keep Alive
        const MINIMUM_DATA_LEN: usize = 12;

        let data_len = data.len();
        if data_len < MINIMUM_DATA_LEN {
            return Err(ProtocolInspectError::NeedMoreData(
                MINIMUM_DATA_LEN - data_len,
            ));
        }

        if data[0] != 0x10 {
            self.exclude_current();
            return Ok(None);
        }

        // exclude impossible protocols
        self.exclude_other(MaybeProtocol::Ssl);
        self.exclude_other(MaybeProtocol::Ssh);
        self.exclude_other(MaybeProtocol::Http);
        self.exclude_other(MaybeProtocol::Rtsp);
        self.exclude_other(MaybeProtocol::Stomp);
        self.exclude_other(MaybeProtocol::Smpp);
        self.exclude_other(MaybeProtocol::Rtmp);
        self.exclude_other(MaybeProtocol::BitTorrent);
        self.exclude_other(MaybeProtocol::Ldap);

        // Remaining Length is an unsigned LEB128, capped at 4 bytes. The
        // protocol name starts after those length bytes.
        let len_src = &data[1..(1 + 4).min(data_len)];
        let (remaining_len, len_bytes) = match Leb128::<u32>::decode(len_src) {
            Ok(decoded) => (decoded.value() as usize, decoded.encoded_len()),
            Err(Leb128DecodeError::NeedMoreData) if len_src.len() < 4 => {
                return Err(ProtocolInspectError::NeedMoreData(1));
            }
            Err(_) => {
                self.exclude_current();
                return Ok(None);
            }
        };
        // Protocol Name, Protocol Level, Connect Flags, and Keep Alive.
        const MINIMUM_VARIABLE_HEADER: usize = 10;
        if remaining_len < MINIMUM_VARIABLE_HEADER {
            self.exclude_current();
            return Ok(None);
        }

        let name_at = 1 + len_bytes;
        let header_end = name_at + 7;
        if data_len < header_end {
            return Err(ProtocolInspectError::NeedMoreData(header_end - data_len));
        }

        if &data[name_at..name_at + 6] != b"\x00\x04MQTT" {
            self.exclude_current();
            return Ok(None);
        }

        let protocol_level = data[name_at + 6];
        match protocol_level {
            0x04 => {}
            0x05 => {}
            _ => {
                self.exclude_current();
                return Ok(None);
            }
        }

        Ok(Some(Protocol::Mqtt))
    }
}
