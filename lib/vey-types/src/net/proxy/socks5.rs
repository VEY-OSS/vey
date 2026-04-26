/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use url::Url;

use super::ProxyParseError;
use crate::net::{Host, SocksAuth, UpstreamAddr};

pub struct Socks5Proxy {
    peer: UpstreamAddr,
    pub auth: SocksAuth,
}

impl Socks5Proxy {
    pub fn peer(&self) -> &UpstreamAddr {
        &self.peer
    }

    pub(super) fn from_url_authority(url: &Url) -> Result<Self, ProxyParseError> {
        let host = url.host().ok_or(ProxyParseError::NoHostFound)?;
        let port = url.port().unwrap_or(1080);

        let host = Host::try_from(host)?;
        let peer = UpstreamAddr::new(host, port);

        let auth = SocksAuth::try_from(url)?;

        Ok(Socks5Proxy { peer, auth })
    }
}
