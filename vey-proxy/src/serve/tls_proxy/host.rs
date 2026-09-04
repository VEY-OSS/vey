/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::Context;

use vey_types::net::{OpensslServerConfig, OpensslTicketKey, RollingTicketer};

use crate::site::{Site, SiteEgress};

pub(crate) struct TlsHost {
    site: Arc<Site>,
    egress: Arc<SiteEgress>,
    tls_server: OpensslServerConfig,
}

impl TlsHost {
    pub(super) fn try_build(
        site: Arc<Site>,
        ticketer: Option<Arc<RollingTicketer<OpensslTicketKey>>>,
    ) -> anyhow::Result<Option<Self>> {
        let Some(builder) = site.tls_server_builder() else {
            return Ok(None);
        };
        let tls_server = builder
            .build_with_alpn_protocols(None, ticketer)
            .context("failed to build tls server")?;

        let egress = Arc::new(SiteEgress::from_site_config(site.config()));
        Ok(Some(TlsHost {
            site,
            egress,
            tls_server,
        }))
    }

    pub(crate) fn site(&self) -> &Arc<Site> {
        &self.site
    }

    #[inline]
    pub(crate) fn egress(&self) -> &Arc<SiteEgress> {
        &self.egress
    }

    pub(crate) fn tls_server(&self) -> &OpensslServerConfig {
        &self.tls_server
    }
}
