/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::Context;

use vey_types::metrics::NodeName;
use vey_types::net::{Host, OpensslClientConfig, RustlsServerConfigBuilder, UpstreamAddr};

use crate::config::site::SiteConfig;

pub(crate) struct Site {
    config: Arc<SiteConfig>,
    tls_client: Option<OpensslClientConfig>,
}

impl Site {
    pub(super) fn try_build(config: &Arc<SiteConfig>) -> anyhow::Result<Self> {
        let tls_client = if let Some(builder) = &config.tls_client_builder {
            let client = builder.build().context("failed to build tls client")?;
            Some(client)
        } else {
            None
        };

        Ok(Site {
            config: Arc::clone(config),
            tls_client,
        })
    }

    pub(crate) fn id(&self) -> &NodeName {
        self.config.id()
    }

    pub(crate) fn upstream(&self) -> &UpstreamAddr {
        self.config.upstream()
    }

    pub(crate) fn tls_name(&self) -> &Host {
        &self.config.tls_name
    }

    pub(crate) fn tls_server_builder(&self) -> Option<&RustlsServerConfigBuilder> {
        self.config.tls_server_builder.as_ref()
    }

    pub(crate) fn tls_client(&self) -> Option<&OpensslClientConfig> {
        self.tls_client.as_ref()
    }
}
