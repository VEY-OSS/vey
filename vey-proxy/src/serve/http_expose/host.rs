/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::Context;

use vey_types::net::{AlpnProtocol, OpensslServerConfig, OpensslTicketKey, RollingTicketer};

use crate::site::Site;

pub(crate) struct HttpHost {
    site: Arc<Site>,
    tls_server: Option<OpensslServerConfig>,
}

impl HttpHost {
    pub(super) fn try_build(
        site: Arc<Site>,
        ticketer: Option<Arc<RollingTicketer<OpensslTicketKey>>>,
    ) -> anyhow::Result<Self> {
        let tls_server = if let Some(builder) = site.tls_server_builder() {
            let server = builder
                .build_with_alpn_protocols(
                    Some(vec![AlpnProtocol::Http11, AlpnProtocol::Http10]),
                    ticketer,
                )
                .context("failed to build tls server")?;
            Some(server)
        } else {
            None
        };

        Ok(HttpHost { site, tls_server })
    }

    #[allow(clippy::unused_self)]
    pub(super) fn new_for_reload(
        &self,
        site: Arc<Site>,
        ticketer: Option<Arc<RollingTicketer<OpensslTicketKey>>>,
    ) -> anyhow::Result<Self> {
        // rebuild tls_server; keep this host's stats / limiter when they are added
        Self::try_build(site, ticketer)
    }

    pub(crate) fn site(&self) -> &Arc<Site> {
        &self.site
    }

    pub(crate) fn tls_server(&self) -> Option<&OpensslServerConfig> {
        self.tls_server.as_ref()
    }
}
