/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
pub struct QuinnEndpointConfig {
    udp_payload_size: Option<u16>,
    connection_id_lifetime: Option<Duration>,
}

impl QuinnEndpointConfig {
    #[inline]
    pub fn udp_payload_size(&self) -> Option<u16> {
        self.udp_payload_size
    }

    #[inline]
    pub fn set_udp_payload_size(&mut self, payload_size: u16) {
        self.udp_payload_size = Some(payload_size.clamp(1200, 65527));
    }

    #[inline]
    pub fn connection_id_lifetime(&self) -> Option<Duration> {
        self.connection_id_lifetime
    }

    #[inline]
    pub fn set_connection_id_lifetime(&mut self, lifetime: Duration) {
        self.connection_id_lifetime = Some(lifetime);
    }
}
