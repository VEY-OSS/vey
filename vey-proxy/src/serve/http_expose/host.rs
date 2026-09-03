/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::Context;

use vey_types::net::{AlpnProtocol, OpensslServerConfig, OpensslTicketKey, RollingTicketer};

use crate::site::{Site, SiteEgress};

pub(crate) struct HttpHost {
    site: Arc<Site>,
    egress: Arc<SiteEgress>,
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

        let egress = Arc::new(SiteEgress::from_site_config(site.config()));
        Ok(HttpHost {
            site,
            egress,
            tls_server,
        })
    }

    pub(crate) fn site(&self) -> &Arc<Site> {
        &self.site
    }

    #[inline]
    pub(crate) fn egress(&self) -> &Arc<SiteEgress> {
        &self.egress
    }

    pub(crate) fn tls_server(&self) -> Option<&OpensslServerConfig> {
        self.tls_server.as_ref()
    }
}
