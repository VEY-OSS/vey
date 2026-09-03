/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::borrow::Cow;

use vey_types::net::{TcpConnectConfig, TcpKeepAliveConfig, TcpMiscSockOpts, UdpMiscSockOpts};
use vey_types::resolve::ResolveStrategy;

use crate::auth::UserContext;
use crate::config::site::SiteConfig;
use crate::escape::EgressPathSelection;

/// Origin-site egress snapshot, filled when `Site` is built.
/// Shrink fields are merged with TenantUser in `SiteContext`; lookup fields
/// stay here and are searched OriginSite then TenantUser.
#[derive(Clone, Default)]
pub(crate) struct SiteEgress {
    resolve_strategy: Option<ResolveStrategy>,
    tcp_connect: Option<TcpConnectConfig>,
    tcp_remote_keepalive: TcpKeepAliveConfig,
    tcp_remote_misc_opts: Option<TcpMiscSockOpts>,
    udp_remote_misc_opts: Option<UdpMiscSockOpts>,
    egress_path_selection: Option<EgressPathSelection>,
}

impl SiteEgress {
    pub(crate) fn from_site_config(config: &SiteConfig) -> Self {
        let egress_path_selection =
            if config.egress_path_id_map.is_empty() && config.egress_path_value_map.is_empty() {
                None
            } else {
                let mut path = EgressPathSelection::default();
                for (escaper, id) in &config.egress_path_id_map {
                    path.set_string_id(escaper.clone(), id.clone());
                }
                for (escaper, value) in &config.egress_path_value_map {
                    path.set_json_value(escaper.clone(), value.clone());
                }
                Some(path)
            };

        SiteEgress {
            resolve_strategy: config.resolve_strategy,
            tcp_connect: config.tcp_connect,
            tcp_remote_keepalive: config.tcp_remote_keepalive,
            tcp_remote_misc_opts: config.tcp_remote_misc_opts,
            udp_remote_misc_opts: config.udp_remote_misc_opts,
            egress_path_selection,
        }
    }

    pub(crate) fn shrink_with_tenant(&self, tenant: Option<&UserContext>) -> Self {
        let Some(tenant) = tenant else {
            return self.clone();
        };
        let cfg = tenant.user_config();

        let tcp_connect = match (cfg.tcp_connect.as_ref(), self.tcp_connect.as_ref()) {
            (Some(tenant), Some(site)) => {
                let mut c = *tenant;
                c.limit_to(site);
                Some(c)
            }
            (Some(tenant), None) => Some(*tenant),
            (None, site) => site.copied(),
        };

        SiteEgress {
            resolve_strategy: self.resolve_strategy,
            tcp_connect,
            tcp_remote_keepalive: cfg
                .tcp_remote_keepalive
                .adjust_to(self.tcp_remote_keepalive),
            tcp_remote_misc_opts: shrink_tcp_misc(
                cfg.tcp_remote_misc_opts.as_ref(),
                self.tcp_remote_misc_opts.as_ref(),
            ),
            udp_remote_misc_opts: shrink_udp_misc(
                cfg.udp_remote_misc_opts,
                self.udp_remote_misc_opts,
            ),
            egress_path_selection: self.egress_path_selection.clone(),
        }
    }

    #[inline]
    pub(crate) fn resolve_strategy(&self) -> Option<ResolveStrategy> {
        self.resolve_strategy
    }

    #[inline]
    pub(crate) fn tcp_connect(&self) -> Option<&TcpConnectConfig> {
        self.tcp_connect.as_ref()
    }

    #[inline]
    pub(crate) fn tcp_remote_keepalive(&self) -> TcpKeepAliveConfig {
        self.tcp_remote_keepalive
    }

    pub(crate) fn tcp_remote_misc_opts<'a>(
        &self,
        base_opts: &'a TcpMiscSockOpts,
    ) -> Cow<'a, TcpMiscSockOpts> {
        match &self.tcp_remote_misc_opts {
            Some(opts) => Cow::Owned(opts.adjust_to(base_opts)),
            None => Cow::Borrowed(base_opts),
        }
    }

    pub(crate) fn udp_remote_misc_opts(&self, base_opts: &UdpMiscSockOpts) -> UdpMiscSockOpts {
        match self.udp_remote_misc_opts {
            Some(opts) => opts.adjust_to(base_opts),
            None => *base_opts,
        }
    }

    #[inline]
    pub(crate) fn path_selection(&self) -> Option<&EgressPathSelection> {
        self.egress_path_selection.as_ref()
    }
}

fn shrink_tcp_misc(
    tenant: Option<&TcpMiscSockOpts>,
    site: Option<&TcpMiscSockOpts>,
) -> Option<TcpMiscSockOpts> {
    match (tenant, site) {
        (Some(tenant), Some(site)) => Some(tenant.adjust_to(site)),
        (Some(tenant), None) => Some(*tenant),
        (None, site) => site.copied(),
    }
}

fn shrink_udp_misc(
    tenant: Option<UdpMiscSockOpts>,
    site: Option<UdpMiscSockOpts>,
) -> Option<UdpMiscSockOpts> {
    match (tenant, site) {
        (Some(tenant), Some(site)) => Some(tenant.adjust_to(&site)),
        (Some(tenant), None) => Some(tenant),
        (None, site) => site,
    }
}
