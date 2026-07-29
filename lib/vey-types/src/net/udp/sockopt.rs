/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use vey_std_ext::core::OptionExt;

#[cfg(target_os = "linux")]
use crate::net::PortRange;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UdpMiscSockOpts {
    pub time_to_live: Option<u8>,
    pub hop_limit: Option<u8>,
    pub type_of_service: Option<u8>,
    #[cfg(not(windows))]
    pub traffic_class: Option<u8>,
    #[cfg(target_os = "linux")]
    pub netfilter_mark: Option<u32>,
    #[cfg(target_os = "linux")]
    pub local_port_range: Option<PortRange>,
    #[cfg(target_os = "freebsd")]
    pub user_cookie: Option<u32>,
    #[cfg(target_os = "openbsd")]
    pub rtable: Option<u32>,
}

impl UdpMiscSockOpts {
    #[must_use]
    pub fn adjust_to(self, other: &Self) -> Self {
        UdpMiscSockOpts {
            time_to_live: self.time_to_live.existed_min(other.time_to_live),
            hop_limit: self.hop_limit.existed_min(other.hop_limit),
            type_of_service: other.type_of_service.or(self.type_of_service),
            #[cfg(not(windows))]
            traffic_class: other.traffic_class.or(self.traffic_class),
            #[cfg(target_os = "linux")]
            netfilter_mark: other.netfilter_mark.or(self.netfilter_mark),
            #[cfg(target_os = "linux")]
            local_port_range: other.local_port_range.or(self.local_port_range),
            #[cfg(target_os = "freebsd")]
            user_cookie: other.user_cookie.or(self.user_cookie),
            #[cfg(target_os = "openbsd")]
            rtable: other.rtable.or(self.rtable),
        }
    }
}
