/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use ascii::AsciiString;
use log::warn;
use yaml_rust::{Yaml, yaml};

use vey_io_ext::StreamCopyConfig;
use vey_tls_ticket::TlsTicketConfig;
use vey_types::acl::AclNetworkRuleBuilder;
use vey_types::metrics::{MetricTagMap, NodeName};
use vey_types::net::{
    HttpForwardedHeaderType, HttpServerId, OpensslServerConfigBuilder, TcpListenConfig,
    TcpMiscSockOpts, TcpSockSpeedLimitConfig,
};
use vey_yaml::YamlDocPosition;

use super::{
    AnyServerConfig, IDLE_CHECK_DEFAULT_DURATION, IDLE_CHECK_DEFAULT_MAX_COUNT,
    IDLE_CHECK_MAXIMUM_DURATION, ServerConfig, ServerConfigDiffAction,
};

const SERVER_CONFIG_TYPE: &str = "HttpGuard";

/// collection of timeout config
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HttpGuardServerTimeoutConfig {
    /// for all protocols: set the idle time to wait before recv of valid request header after all tasks done
    pub(crate) recv_req_header: Duration,
    /// for http forward only: the max time to wait after request sent before recv response header
    pub(crate) recv_rsp_header: Duration,
}

impl Default for HttpGuardServerTimeoutConfig {
    fn default() -> Self {
        HttpGuardServerTimeoutConfig {
            recv_req_header: Duration::from_secs(30),
            recv_rsp_header: Duration::from_secs(60),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HttpGuardH1Config {
    pub(crate) pipeline_size: NonZeroUsize,
    pub(crate) pipeline_read_idle_timeout: Duration,
    pub(crate) body_line_max_len: usize,
}

impl Default for HttpGuardH1Config {
    fn default() -> Self {
        HttpGuardH1Config {
            pipeline_size: NonZeroUsize::new(10).unwrap(),
            pipeline_read_idle_timeout: Duration::from_secs(300),
            body_line_max_len: 8192,
        }
    }
}

impl HttpGuardH1Config {
    fn parse_yaml(&mut self, value: &Yaml) -> anyhow::Result<()> {
        let Yaml::Hash(map) = value else {
            return Err(anyhow!("yaml value type for 'h1' should be 'map'"));
        };
        vey_yaml::foreach_kv(map, |k, v| self.set(k, v))
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "pipeline_size" => {
                self.pipeline_size = vey_yaml::value::as_nonzero_usize(v)
                    .context(format!("invalid nonzero usize value for key {k}"))?;
                Ok(())
            }
            "pipeline_read_idle_timeout" => {
                self.pipeline_read_idle_timeout = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "body_line_max_length" => {
                self.body_line_max_len = vey_yaml::value::as_usize(v)
                    .context(format!("invalid usize value for key {k}"))?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HttpGuardH2Config {
    pub(crate) max_header_list_size: u32,
    pub(crate) max_concurrent_streams: u32,
    stream_window_size: u32,
    connection_window_size: u32,
    max_frame_size: u32,
    pub(crate) max_send_buffer_size: usize,
    pub(crate) upstream_handshake_timeout: Duration,
    pub(crate) upstream_stream_open_timeout: Duration,
    pub(crate) client_handshake_timeout: Duration,
    pub(crate) ping_interval: Duration,
}

impl Default for HttpGuardH2Config {
    fn default() -> Self {
        HttpGuardH2Config {
            max_header_list_size: 64 * 1024,
            max_concurrent_streams: 128,
            stream_window_size: 1024 * 1024,
            connection_window_size: 2 * 1024 * 1024,
            max_frame_size: 256 * 1024,
            max_send_buffer_size: 8 * 1024 * 1024,
            upstream_handshake_timeout: Duration::from_secs(10),
            upstream_stream_open_timeout: Duration::from_secs(10),
            client_handshake_timeout: Duration::from_secs(4),
            ping_interval: Duration::from_secs(60),
        }
    }
}

impl HttpGuardH2Config {
    pub(crate) fn apply_to_server_builder(&self, builder: &mut h2::server::Builder) {
        builder
            .max_header_list_size(self.max_header_list_size)
            .max_concurrent_streams(self.max_concurrent_streams)
            .max_frame_size(self.max_frame_size)
            .max_send_buffer_size(self.max_send_buffer_size)
            .initial_window_size(self.stream_window_size)
            .initial_connection_window_size(self.connection_window_size)
            .enable_connect_protocol();
    }

    pub(crate) fn apply_to_client_builder(&self, builder: &mut h2::client::Builder) {
        builder
            .enable_push(false)
            .max_header_list_size(self.max_header_list_size)
            .max_concurrent_streams(0)
            .max_frame_size(self.max_frame_size)
            .max_send_buffer_size(self.max_send_buffer_size)
            .initial_window_size(self.stream_window_size)
            .initial_connection_window_size(self.connection_window_size);
    }

    fn parse_yaml(&mut self, value: &Yaml) -> anyhow::Result<()> {
        let Yaml::Hash(map) = value else {
            return Err(anyhow!("yaml value type for 'h2' should be 'map'"));
        };
        vey_yaml::foreach_kv(map, |k, v| self.set(k, v))
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "max_header_list_size" | "max_header_size" => {
                self.max_header_list_size = vey_yaml::humanize::as_u32(v)
                    .context(format!("invalid humanize u32 value for key {k}"))?;
                Ok(())
            }
            "max_concurrent_streams" => {
                self.max_concurrent_streams = vey_yaml::value::as_u32(v)?;
                Ok(())
            }
            "max_frame_size" => {
                let size = vey_yaml::humanize::as_u32(v)
                    .context(format!("invalid humanize u32 value for key {k}"))?;
                self.max_frame_size = size.clamp(1 << 14, (1 << 24) - 1);
                Ok(())
            }
            "stream_window_size" => {
                let size = vey_yaml::humanize::as_u32(v)
                    .context(format!("invalid humanize u32 value for key {k}"))?;
                self.stream_window_size = size.max(65536);
                Ok(())
            }
            "connection_window_size" => {
                let size = vey_yaml::humanize::as_u32(v)
                    .context(format!("invalid humanize u32 value for key {k}"))?;
                self.connection_window_size = size.max(65536);
                Ok(())
            }
            "max_send_buffer_size" => {
                self.max_send_buffer_size = vey_yaml::humanize::as_usize(v)
                    .context(format!("invalid humanize usize value for key {k}"))?;
                Ok(())
            }
            "upstream_handshake_timeout" => {
                self.upstream_handshake_timeout = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "upstream_stream_open_timeout" => {
                self.upstream_stream_open_timeout = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "client_handshake_timeout" => {
                self.client_handshake_timeout = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "ping_interval" => {
                self.ping_interval = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HttpGuardServerConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) escaper: NodeName,
    pub(crate) auditor: NodeName,
    pub(crate) site_group: NodeName,
    pub(crate) shared_logger: Option<AsciiString>,
    pub(crate) listen: Option<TcpListenConfig>,
    pub(crate) listen_in_worker: bool,
    pub(crate) ingress_net_filter: Option<AclNetworkRuleBuilder>,
    pub(crate) tcp_sock_speed_limit: TcpSockSpeedLimitConfig,
    pub(crate) task_idle_check_interval: Duration,
    pub(crate) task_idle_max_count: usize,
    pub(crate) flush_task_log_on_created: bool,
    pub(crate) flush_task_log_on_connected: bool,
    pub(crate) task_log_flush_interval: Option<Duration>,
    pub(crate) tcp_copy: StreamCopyConfig,
    pub(crate) tcp_misc_opts: TcpMiscSockOpts,
    pub(crate) server_id: Option<HttpServerId>,
    pub(crate) no_proxy_status: bool,
    pub(crate) timeout: HttpGuardServerTimeoutConfig,
    pub(crate) req_hdr_max_size: usize,
    pub(crate) rsp_hdr_max_size: usize,
    pub(crate) log_uri_max_chars: usize,
    pub(crate) no_early_error_reply: bool,
    pub(crate) append_forwarded_for: HttpForwardedHeaderType,
    pub(crate) h1: HttpGuardH1Config,
    pub(crate) h2: HttpGuardH2Config,
    pub(crate) extra_metrics_tags: Option<Arc<MetricTagMap>>,
    pub(crate) global_tls_server: Option<OpensslServerConfigBuilder>,
    pub(crate) tls_ticketer: Option<TlsTicketConfig>,
    pub(crate) client_hello_recv_timeout: Duration,
}

impl HttpGuardServerConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        HttpGuardServerConfig {
            name: NodeName::default(),
            position,
            escaper: NodeName::default(),
            auditor: NodeName::default(),
            site_group: NodeName::default(),
            shared_logger: None,
            listen: None,
            listen_in_worker: false,
            ingress_net_filter: None,
            tcp_sock_speed_limit: TcpSockSpeedLimitConfig::default(),
            task_idle_check_interval: IDLE_CHECK_DEFAULT_DURATION,
            task_idle_max_count: IDLE_CHECK_DEFAULT_MAX_COUNT,
            flush_task_log_on_created: false,
            flush_task_log_on_connected: false,
            task_log_flush_interval: None,
            tcp_copy: Default::default(),
            tcp_misc_opts: Default::default(),
            server_id: None,
            no_proxy_status: false,
            timeout: HttpGuardServerTimeoutConfig::default(),
            req_hdr_max_size: 65536, // 64KiB
            rsp_hdr_max_size: 65536, // 64KiB
            log_uri_max_chars: 1024,
            no_early_error_reply: false,
            append_forwarded_for: HttpForwardedHeaderType::default(),
            h1: HttpGuardH1Config::default(),
            h2: HttpGuardH2Config::default(),
            extra_metrics_tags: None,
            global_tls_server: None,
            tls_ticketer: None,
            client_hello_recv_timeout: Duration::from_secs(1),
        }
    }

    pub(crate) fn parse(
        map: &yaml::Hash,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut server = HttpGuardServerConfig::new(position);

        vey_yaml::foreach_kv(map, |k, v| server.set(k, v))?;

        server.check()?;
        Ok(server)
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            super::CONFIG_KEY_SERVER_TYPE => Ok(()),
            super::CONFIG_KEY_SERVER_NAME => {
                self.name = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "escaper" => {
                self.escaper = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "auditor" => {
                self.auditor = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "site_group" => {
                self.site_group = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "shared_logger" => {
                let name = vey_yaml::value::as_ascii(v)?;
                self.shared_logger = Some(name);
                Ok(())
            }
            "extra_metrics_tags" => {
                let tags = vey_yaml::value::as_static_metrics_tags(v)
                    .context(format!("invalid static metrics tags value for key {k}"))?;
                self.extra_metrics_tags = Some(Arc::new(tags));
                Ok(())
            }
            "listen" => {
                let config = vey_yaml::value::as_tcp_listen_config(v)
                    .context(format!("invalid tcp listen config value for key {k}"))?;
                self.listen = Some(config);
                Ok(())
            }
            "listen_in_worker" => {
                self.listen_in_worker = vey_yaml::value::as_bool(v)?;
                Ok(())
            }
            "ingress_network_filter" | "ingress_net_filter" => {
                let filter = vey_yaml::value::acl::as_ingress_network_rule_builder(v).context(
                    format!("invalid ingress network acl rule value for key {k}"),
                )?;
                self.ingress_net_filter = Some(filter);
                Ok(())
            }
            "server_id" => {
                self.server_id = Some(
                    vey_yaml::value::as_http_server_id(v)
                        .context(format!("invalid http server id value for key {k}"))?,
                );
                Ok(())
            }
            "no_proxy_status" => {
                self.no_proxy_status = vey_yaml::value::as_bool(v)
                    .context(format!("invalid bool value for key {k}"))?;
                Ok(())
            }
            "req_header_recv_timeout" => {
                self.timeout.recv_req_header = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "rsp_header_recv_timeout" => {
                self.timeout.recv_rsp_header = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "req_header_max_size" => {
                self.req_hdr_max_size = vey_yaml::humanize::as_usize(v)
                    .context(format!("invalid humanize usize value for key {k}"))?;
                Ok(())
            }
            "rsp_header_max_size" => {
                self.rsp_hdr_max_size = vey_yaml::humanize::as_usize(v)
                    .context(format!("invalid humanize usize value for key {k}"))?;
                Ok(())
            }
            "log_uri_max_chars" | "uri_log_max_chars" => {
                self.log_uri_max_chars = vey_yaml::value::as_usize(v)
                    .context(format!("invalid usize value for key {k}"))?;
                Ok(())
            }
            "no_early_error_reply" => {
                self.no_early_error_reply = vey_yaml::value::as_bool(v)
                    .context(format!("invalid bool value for key {k}"))?;
                Ok(())
            }
            "append_forwarded_for" => {
                self.append_forwarded_for = vey_yaml::value::as_http_forwarded_header_type(v)
                    .context(format!(
                        "invalid http forwarded header type value for key {k}"
                    ))?;
                Ok(())
            }
            "h1" => self.h1.parse_yaml(v),
            "h2" => self.h2.parse_yaml(v),
            "tcp_sock_speed_limit" => {
                self.tcp_sock_speed_limit = vey_yaml::value::as_tcp_sock_speed_limit(v)
                    .context(format!("invalid tcp socket speed limit value for key {k}"))?;
                Ok(())
            }
            "tcp_conn_speed_limit" | "tcp_conn_limit" | "conn_limit" => {
                warn!("deprecated config key '{k}', please use 'tcp_sock_speed_limit' instead");
                self.set("tcp_sock_speed_limit", v)
            }
            "tcp_copy_buffer_size" => {
                let buffer_size = vey_yaml::humanize::as_usize(v)
                    .context(format!("invalid humanize usize value for key {k}"))?;
                self.tcp_copy.set_buffer_size(buffer_size);
                Ok(())
            }
            "tcp_copy_yield_size" => {
                let yield_size = vey_yaml::humanize::as_usize(v)
                    .context(format!("invalid humanize usize value for key {k}"))?;
                self.tcp_copy.set_yield_size(yield_size);
                Ok(())
            }
            "tcp_misc_opts" => {
                self.tcp_misc_opts = vey_yaml::value::as_tcp_misc_sock_opts(v)
                    .context(format!("invalid tcp misc sock opts value for key {k}"))?;
                Ok(())
            }
            "task_idle_check_duration" => {
                warn!("deprecated config key '{k}', please use 'task_idle_check_interval' instead");
                self.set("task_idle_check_interval", v)
            }
            "task_idle_check_interval" => {
                self.task_idle_check_interval = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "task_idle_max_count" => {
                self.task_idle_max_count = vey_yaml::value::as_usize(v)
                    .context(format!("invalid usize value for key {k}"))?;
                Ok(())
            }
            "flush_task_log_on_created" => {
                self.flush_task_log_on_created = vey_yaml::value::as_bool(v)?;
                Ok(())
            }
            "flush_task_log_on_connected" => {
                self.flush_task_log_on_connected = vey_yaml::value::as_bool(v)?;
                Ok(())
            }
            "task_log_flush_interval" => {
                let interval = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                self.task_log_flush_interval = Some(interval);
                Ok(())
            }
            "global_tls_server" => {
                let lookup_dir = vey_daemon::config::get_lookup_dir(self.position.as_ref())?;
                let builder =
                    vey_yaml::value::as_openssl_tls_server_config_builder(v, Some(lookup_dir))
                        .context(format!(
                            "invalid openssl tls server config builder value for key {k}"
                        ))?;
                self.global_tls_server = Some(builder);
                Ok(())
            }
            "tls_ticketer" => {
                let lookup_dir = vey_daemon::config::get_lookup_dir(self.position.as_ref())?;
                let ticketer = TlsTicketConfig::parse_yaml(v, Some(lookup_dir))
                    .context(format!("invalid tls ticket config value for key {k}"))?;
                self.tls_ticketer = Some(ticketer);
                Ok(())
            }
            "client_hello_recv_timeout" => {
                self.client_hello_recv_timeout = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    fn check(&mut self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        if self.escaper.is_empty() {
            return Err(anyhow!("escaper is not set"));
        }
        if self.site_group.is_empty() {
            return Err(anyhow!("site_group is not set"));
        }
        if self.task_idle_check_interval > IDLE_CHECK_MAXIMUM_DURATION {
            self.task_idle_check_interval = IDLE_CHECK_MAXIMUM_DURATION;
        }

        Ok(())
    }
}

impl ServerConfig for HttpGuardServerConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    fn r#type(&self) -> &'static str {
        SERVER_CONFIG_TYPE
    }

    fn escaper(&self) -> &NodeName {
        &self.escaper
    }

    fn user_group(&self) -> &NodeName {
        Default::default()
    }

    fn auditor(&self) -> &NodeName {
        &self.auditor
    }

    fn diff_action(&self, new: &AnyServerConfig) -> ServerConfigDiffAction {
        let AnyServerConfig::HttpGuard(new) = new else {
            return ServerConfigDiffAction::SpawnNew;
        };

        if self.eq(new) {
            return ServerConfigDiffAction::NoAction;
        }

        if self.listen != new.listen {
            return ServerConfigDiffAction::ReloadAndRespawn;
        }

        ServerConfigDiffAction::ReloadNoRespawn
    }

    fn shared_logger(&self) -> Option<&str> {
        self.shared_logger.as_ref().map(|s| s.as_str())
    }

    fn task_log_flush_interval(&self) -> Option<Duration> {
        self.task_log_flush_interval
    }

    #[inline]
    fn limited_copy_config(&self) -> StreamCopyConfig {
        self.tcp_copy
    }

    #[inline]
    fn task_max_idle_count(&self) -> usize {
        self.task_idle_max_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yaml_rust::YamlLoader;

    #[test]
    fn reject_hosts_with_site_group() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local_in
type: http_guard
listen: "[::]:8080"
escaper: default
site_group: local
hosts:
  - exact_match: app.internal
    upstream: 127.0.0.1:8080
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        assert!(HttpGuardServerConfig::parse(map, None).is_err());
    }

    #[test]
    fn parse_site_group_and_auditor() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: http_in
type: http_guard
listen: "[::]:8080"
escaper: default
site_group: saas
auditor: icap_waf
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let server = HttpGuardServerConfig::parse(map, None).unwrap();
        assert_eq!(server.site_group.as_str(), "saas");
        assert_eq!(server.auditor.as_str(), "icap_waf");
        assert!(server.user_group().is_empty());
    }

    #[test]
    fn reject_user_group() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: http_in
type: http_guard
listen: "[::]:8080"
escaper: default
site_group: saas
user_group: visitors
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        assert!(HttpGuardServerConfig::parse(map, None).is_err());
    }

    #[test]
    fn parse_http_h1_h2() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: http_in
type: http_guard
listen: "[::]:8080"
escaper: default
site_group: saas
req_header_max_size: 32Ki
append_forwarded_for: disable
h1:
  pipeline_size: 4
h2:
  max_concurrent_streams: 32
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let server = HttpGuardServerConfig::parse(map, None).unwrap();
        assert_eq!(server.req_hdr_max_size, 32 * 1024);
        assert_eq!(
            server.append_forwarded_for,
            vey_types::net::HttpForwardedHeaderType::Disable
        );
        assert_eq!(server.h1.pipeline_size.get(), 4);
        assert_eq!(server.h2.max_concurrent_streams, 32);
    }

    #[test]
    fn reject_legacy_top_level_http_keys() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: http_in
type: http_guard
listen: "[::]:8080"
escaper: default
site_group: saas
pipeline_size: 4
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        assert!(HttpGuardServerConfig::parse(map, None).is_err());
    }

    #[test]
    fn reject_nested_http_map() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: http_in
type: http_guard
listen: "[::]:8080"
escaper: default
site_group: saas
http:
  req_header_max_size: 32Ki
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        assert!(HttpGuardServerConfig::parse(map, None).is_err());
    }
}
