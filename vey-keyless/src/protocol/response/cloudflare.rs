/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

//! Cloudflare Keyless wire encode for response types.

use super::KeylessResponse;

const MESSAGE_HEADER_LENGTH: usize = 8;
const ITEM_HEADER_LENGTH: usize = 3;
const BUF_PREFIX_LEN: usize =
    MESSAGE_HEADER_LENGTH + ITEM_HEADER_LENGTH + 1 + ITEM_HEADER_LENGTH;

impl KeylessResponse {
    /// Encode this response into the Cloudflare Keyless wire format.
    pub(crate) fn cloudflare_message(&self) -> Vec<u8> {
        match self {
            KeylessResponse::Data(d) => encode_data(d.id, &d.payload, 0xF0),
            KeylessResponse::Pong(p) => encode_data(p.id, &p.payload, 0xF2),
            KeylessResponse::Error(e) => encode_error(e.id, e.code as u8),
        }
    }
}

fn encode_data(id: u32, payload: &[u8], opcode: u8) -> Vec<u8> {
    let item_len = payload.len() as u16;
    let item_len_h = (item_len >> 8) as u8;
    let item_len_l = (item_len & 0xFF) as u8;

    let msg_len = (payload.len() + BUF_PREFIX_LEN - MESSAGE_HEADER_LENGTH) as u16;
    let msg_len_h = (msg_len >> 8) as u8;
    let msg_len_l = (msg_len & 0xFF) as u8;

    let b = id.to_be_bytes();
    let prefix: [u8; BUF_PREFIX_LEN] = [
        0x01, 0x00, // protocol version
        msg_len_h, msg_len_l, // message length
        b[0], b[1], b[2], b[3], // message id
        0x11, 0x00, 0x01, opcode, // OpCode
        0x12, item_len_h, item_len_l, // Payload
    ];
    let mut buf = Vec::with_capacity(payload.len() + BUF_PREFIX_LEN);
    buf.extend_from_slice(&prefix);
    buf.extend_from_slice(payload);
    buf
}

fn encode_error(id: u32, code: u8) -> Vec<u8> {
    let b = id.to_be_bytes();
    vec![
        0x01, 0x00, // protocol version
        0x00, 0x08, // message length
        b[0], b[1], b[2], b[3], // message id
        0x11, 0x00, 0x01, 0xFF, // OpCode
        0x12, 0x00, 0x01, code, // Payload
    ]
}
