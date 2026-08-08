/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;

use vey_types::net::{OpensslTicketKey, RollingTicketKey, RollingTicketer};

use super::{TicketKeyUpdate, TicketSourceConfig};

#[cfg(feature = "yaml")]
mod yaml;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TlsTicketConfig {
    pub(crate) check_interval: Duration,
    pub(crate) local_lifetime: u32,
    pub(crate) remote_source: Option<TicketSourceConfig>,
}

impl Default for TlsTicketConfig {
    fn default() -> Self {
        TlsTicketConfig {
            check_interval: Duration::from_secs(300),
            local_lifetime: 12 * 60 * 60, // 12h
            remote_source: None,
        }
    }
}

impl TlsTicketConfig {
    pub fn build_and_spawn_updater(
        &self,
    ) -> anyhow::Result<Arc<RollingTicketer<OpensslTicketKey>>> {
        let initial_key = OpensslTicketKey::new_random(self.local_lifetime)
            .context("failed to create initial random key")?;
        let ticketer = Arc::new(RollingTicketer::new(initial_key));
        TicketKeyUpdate::new(self.clone(), ticketer.clone()).spawn_run();
        Ok(ticketer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let config = TlsTicketConfig::default();
        assert_eq!(config.check_interval, Duration::from_secs(300));
        assert_eq!(config.local_lifetime, 12 * 60 * 60);
        assert!(config.remote_source.is_none());
    }

    #[test]
    fn config_clone_is_equal() {
        let config = TlsTicketConfig::default();
        assert_eq!(config, config.clone());
    }
}
