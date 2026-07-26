/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use vey_dpi::Protocol;

const EXP_PDU_TAG_DISSECTOR_NAME: u8 = 12;
const EXP_PDU_TAG_DISSECTOR_TABLE_NAME: u8 = 14;
const EXP_PDU_TAG_DISSECTOR_TABLE_NAME_NUM_VAL: u8 = 32;

const EXP_PDU_TAG_DISSECTOR_TABLE_NUM_VAL_LEN: u8 = 4;

#[derive(Clone, Copy)]
pub enum ExportedPduDissectorHint {
    Protocol(Protocol),
    TcpPort(u16),
    TlsPort(u16),
}

impl ExportedPduDissectorHint {
    pub(crate) fn serialize_exported_pdu(buf: &mut Vec<u8>) {
        buf.extend_from_slice(&[0x00, EXP_PDU_TAG_DISSECTOR_NAME]);
        buf.extend_from_slice(&[0x00, 0x0C]);
        buf.extend_from_slice(b"exported_pdu");
    }

    pub(crate) fn serialize(&self, buf: &mut Vec<u8>) {
        match self {
            ExportedPduDissectorHint::Protocol(protocol) => {
                let dissector = protocol.wireshark_dissector();
                if !dissector.is_empty() {
                    buf.extend_from_slice(&[0x00, EXP_PDU_TAG_DISSECTOR_NAME]);
                    let len = (dissector.len() & 0xFFFF) as u16;
                    buf.extend_from_slice(&len.to_be_bytes());
                    buf.extend_from_slice(dissector.as_bytes());
                }
            }
            ExportedPduDissectorHint::TcpPort(port) => {
                buf.extend_from_slice(&[
                    0x00,
                    EXP_PDU_TAG_DISSECTOR_TABLE_NAME,
                    0x00,
                    0x08,
                    b't',
                    b'c',
                    b'p',
                    b'.',
                    b'p',
                    b'o',
                    b'r',
                    b't',
                ]);
                serialize_dissector_table_name_num_val(buf, *port);
            }
            ExportedPduDissectorHint::TlsPort(port) => {
                buf.extend_from_slice(&[
                    0x00,
                    EXP_PDU_TAG_DISSECTOR_TABLE_NAME,
                    0x00,
                    0x08,
                    b't',
                    b'l',
                    b's',
                    b'.',
                    b'p',
                    b'o',
                    b'r',
                    b't',
                ]);
                serialize_dissector_table_name_num_val(buf, *port);
            }
        }
    }
}

fn serialize_dissector_table_name_num_val(buf: &mut Vec<u8>, port: u16) {
    let port = port.to_be_bytes();
    buf.extend_from_slice(&[
        0x00,
        EXP_PDU_TAG_DISSECTOR_TABLE_NAME_NUM_VAL,
        0x00,
        EXP_PDU_TAG_DISSECTOR_TABLE_NUM_VAL_LEN,
        0x00,
        0x00,
        port[0],
        port[1],
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use vey_dpi::Protocol;

    #[test]
    fn serialize_exported_pdu_prefix() {
        let mut buf = Vec::new();
        ExportedPduDissectorHint::serialize_exported_pdu(&mut buf);
        assert!(buf.windows(12).any(|w| w == b"exported_pdu"));
    }

    #[test]
    fn serialize_tcp_port_hint() {
        let mut buf = Vec::new();
        ExportedPduDissectorHint::TcpPort(443).serialize(&mut buf);
        assert!(buf.windows(8).any(|w| w == b"tcp.port"));
        assert!(buf.ends_with(&443u16.to_be_bytes()));
    }

    #[test]
    fn serialize_tls_port_hint() {
        let mut buf = Vec::new();
        ExportedPduDissectorHint::TlsPort(8443).serialize(&mut buf);
        assert!(buf.windows(8).any(|w| w == b"tls.port"));
        assert!(buf.ends_with(&8443u16.to_be_bytes()));
    }

    #[test]
    fn serialize_protocol_hint_http() {
        let mut buf = Vec::new();
        ExportedPduDissectorHint::Protocol(Protocol::Http1).serialize(&mut buf);
        let dissector = Protocol::Http1.wireshark_dissector();
        assert!(!dissector.is_empty());
        assert!(buf.windows(dissector.len()).any(|w| w == dissector.as_bytes()));
    }
}
