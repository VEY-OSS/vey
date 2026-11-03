/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use super::{MaybeProtocol, Protocol, ProtocolInspectError, ProtocolInspectState};

impl ProtocolInspectState {
    pub(crate) fn check_rtsp_client_setup_request(
        &mut self,
        data: &[u8],
    ) -> Result<Option<Protocol>, ProtocolInspectError> {
        macro_rules! check_method {
            ($m:literal) => {
                if data.starts_with($m) {
                    return self.check_rtsp_after_method(data, $m.len());
                }
            };
        }

        // at least SETUP rtsp://<x> RTSP/<N>.0\r\n
        const MINIMUM_DATA_LEN: usize = 25;

        let data_len = data.len();
        if data_len < MINIMUM_DATA_LEN {
            return Err(ProtocolInspectError::NeedMoreData(
                MINIMUM_DATA_LEN - data_len,
            ));
        }

        match data[0] {
            b'A' => {
                // 0x41
                self.rtsp_exclude_by_byte0();
                check_method!(b"ANNOUNCE ");
            }
            b'D' => {
                // 0x44
                self.rtsp_exclude_by_byte0();
                check_method!(b"DESCRIBE ");
            }
            b'G' => {
                // 0x47
                self.rtsp_exclude_by_byte0();
                check_method!(b"GET_PARAMETER ");
            }
            b'O' => {
                // 0x4F
                self.rtsp_exclude_by_byte0();
                check_method!(b"OPTIONS ");
            }
            b'P' => {
                // 0x50
                self.rtsp_exclude_by_byte0();
                check_method!(b"PAUSE ");
                check_method!(b"PLAY ");
            }
            b'R' => {
                // 0x52
                self.rtsp_exclude_by_byte0();
                check_method!(b"RECORD ");
                check_method!(b"REDIRECT ");
            }
            b'S' => {
                // 0x53
                self.rtsp_exclude_by_byte0();
                check_method!(b"SET_PARAMETER ");
                check_method!(b"SETUP ");
            }
            b'T' => {
                // 0x54
                self.rtsp_exclude_by_byte0();
                check_method!(b"TEARDOWN ");
            }
            _ => {}
        }

        self.exclude_current();
        Ok(None)
    }

    fn rtsp_exclude_by_byte0(&mut self) {
        self.exclude_other(MaybeProtocol::Ssl);
        self.exclude_other(MaybeProtocol::Mqtt);
        self.exclude_other(MaybeProtocol::Smpp);
        self.exclude_other(MaybeProtocol::Rtmp);
        self.exclude_other(MaybeProtocol::BitTorrent);
        self.exclude_other(MaybeProtocol::Ldap);
    }

    fn check_rtsp_after_method(
        &mut self,
        data: &[u8],
        uri_offset: usize,
    ) -> Result<Option<Protocol>, ProtocolInspectError> {
        let rest = &data[uri_offset..];
        let rtsp_uri = rest.starts_with(b"rtsp://");
        let options_star = data.starts_with(b"OPTIONS ") && rest.starts_with(b"* RTSP/");
        if !rtsp_uri && !options_star {
            self.exclude_current();
            return Ok(None);
        }

        self.exclude_other(MaybeProtocol::Ssh);
        self.exclude_other(MaybeProtocol::Http);
        self.exclude_other(MaybeProtocol::Stomp);

        Ok(Some(Protocol::Rtsp))
    }
}
