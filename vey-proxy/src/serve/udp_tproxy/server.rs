/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use arc_swap::{ArcSwap, ArcSwapOption};
use async_trait::async_trait;
#[cfg(feature = "quic")]
use quinn::Connection;
use slog::Logger;
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio_rustls::server::TlsStream;

use vey_daemon::listen::{
    AcceptQuicServer, AcceptTcpServer, AcceptUdpServer, AcceptedUdpPacketReceiver,
    AcceptedUdpPacketSender, ListenStats, ListenUdpRuntime,
};
use vey_daemon::server::{BaseServer, ClientConnectionInfo, ServerReloadCommand};
use vey_io_ext::IdleWheel;
use vey_openssl::SslStream;
use vey_types::acl::{AclAction, AclNetworkRule};
use vey_types::auth::FactsMatchType;
use vey_types::metrics::NodeName;

use super::common::CommonTaskContext;
use super::task::UdpTProxyTask;
use crate::auth::{FactsUserGroup, UserContext, UserGroup};
use crate::config::server::udp_tproxy::UdpTProxyServerConfig;
use crate::config::server::{AnyServerConfig, ServerConfig};
use crate::escape::ArcEscaper;
use crate::serve::udp_stream::UdpStreamServerStats;
use crate::serve::{
    ArcServer, ArcServerInternal, ArcServerStats, Server, ServerInternal, ServerQuitPolicy,
    ServerRegistry, ServerStats, ServerTaskNotes, WrapArcServer,
};

pub(crate) struct UdpTProxyServer {
    config: Arc<UdpTProxyServerConfig>,
    server_stats: Arc<UdpStreamServerStats>,
    listen_stats: Arc<ListenStats>,
    ingress_net_filter: Option<AclNetworkRule>,
    reload_sender: broadcast::Sender<ServerReloadCommand>,
    task_logger: Option<Logger>,

    escaper: ArcSwap<ArcEscaper>,
    user_group: ArcSwapOption<FactsUserGroup>,
    quit_policy: Arc<ServerQuitPolicy>,
    idle_wheel: Arc<IdleWheel>,
    reload_version: usize,
}

impl UdpTProxyServer {
    fn new(
        config: Arc<UdpTProxyServerConfig>,
        server_stats: Arc<UdpStreamServerStats>,
        listen_stats: Arc<ListenStats>,
        version: usize,
    ) -> anyhow::Result<Self> {
        let reload_sender = crate::serve::new_reload_notify_channel();
        let ingress_net_filter = config
            .ingress_net_filter
            .as_ref()
            .map(|builder| builder.build());
        let task_logger = config.get_task_logger();
        let idle_wheel = IdleWheel::spawn(config.task_idle_check_interval);

        server_stats.set_extra_tags(config.extra_metrics_tags.clone());

        let escaper = Arc::new(crate::escape::get_or_insert_default(config.escaper()));

        let server = UdpTProxyServer {
            config,
            server_stats,
            listen_stats,
            ingress_net_filter,
            reload_sender,
            task_logger,
            escaper: ArcSwap::new(escaper),
            user_group: ArcSwapOption::new(None),
            quit_policy: Arc::new(ServerQuitPolicy::default()),
            idle_wheel,
            reload_version: version,
        };
        server._update_user_group_in_place();
        Ok(server)
    }

    pub(crate) fn prepare_initial(
        config: UdpTProxyServerConfig,
    ) -> anyhow::Result<ArcServerInternal> {
        let config = Arc::new(config);
        let server_stats = Arc::new(UdpStreamServerStats::new(config.name()));
        let listen_stats = Arc::new(ListenStats::new(config.name()));
        let server = UdpTProxyServer::new(config, server_stats, listen_stats, 1)?;
        Ok(Arc::new(server))
    }

    fn prepare_reload(&self, config: AnyServerConfig) -> anyhow::Result<Self> {
        if let AnyServerConfig::UdpTProxy(config) = config {
            let config = Arc::new(config);
            let server_stats = Arc::clone(&self.server_stats);
            let listen_stats = Arc::clone(&self.listen_stats);
            let server =
                UdpTProxyServer::new(config, server_stats, listen_stats, self.reload_version + 1)?;
            Ok(server)
        } else {
            Err(anyhow!(
                "config type mismatch: expect {}, actual {}",
                self.config.r#type(),
                config.r#type()
            ))
        }
    }

    fn drop_early(&self, client_addr: SocketAddr) -> bool {
        if let Some(ingress_net_filter) = &self.ingress_net_filter {
            let (_, action) = ingress_net_filter.check(client_addr.ip());
            match action {
                AclAction::Permit | AclAction::PermitAndLog => {}
                AclAction::Forbid | AclAction::ForbidAndLog => {
                    self.listen_stats.add_dropped();
                    return true;
                }
            }
        }
        false
    }

    fn build_task_notes(&self, cc_info: &ClientConnectionInfo) -> Option<ServerTaskNotes> {
        if let Some(auth_match) = self.config.auth_match {
            let ip = match auth_match {
                FactsMatchType::ClientIp => cc_info.client_ip(),
                FactsMatchType::ServerIp => cc_info.server_ip(),
                FactsMatchType::ServerName => return None,
            };
            let (user, user_type) = self
                .user_group
                .load()
                .as_ref()
                .and_then(|g| g.get_user_by_ip(ip))?;
            let user_ctx = UserContext::new(
                None,
                user,
                user_type,
                self.config.name(),
                self.server_stats.share_extra_tags(),
            );
            if user_ctx.check_client_addr(cc_info.client_addr()).is_err() {
                return None;
            }
            Some(ServerTaskNotes::new(
                cc_info.clone(),
                Some(user_ctx),
                Duration::ZERO,
            ))
        } else {
            Some(ServerTaskNotes::new(cc_info.clone(), None, Duration::ZERO))
        }
    }
}

impl ServerInternal for UdpTProxyServer {
    fn _clone_config(&self) -> AnyServerConfig {
        AnyServerConfig::UdpTProxy(self.config.as_ref().clone())
    }

    fn _depend_on_server(&self, _name: &NodeName) -> bool {
        false
    }

    fn _reload_config_notify_runtime(&self) {
        let cmd = ServerReloadCommand::ReloadVersion(self.reload_version);
        let _ = self.reload_sender.send(cmd);
    }

    fn _update_next_servers_in_place(&self) {}

    fn _update_escaper_in_place(&self) {
        let escaper = crate::escape::get_or_insert_default(self.config.escaper());
        self.escaper.store(Arc::new(escaper));
    }

    fn _update_user_group_in_place(&self) {
        let user_group = if let Some(g) = self.config.get_user_group() {
            let g_type = g.r#type();
            if let UserGroup::Facts(g) = g {
                Some(g)
            } else {
                log::warn!(
                    "server {}: user group {}(type {g_type}) ignored",
                    self.config.name(),
                    self.config.user_group
                );
                None
            }
        } else {
            None
        };
        self.user_group.store(user_group);
    }

    fn _update_audit_handle_in_place(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn _reload_with_old_notifier(
        &self,
        config: AnyServerConfig,
        _registry: &mut ServerRegistry,
    ) -> anyhow::Result<ArcServerInternal> {
        let mut server = self.prepare_reload(config)?;
        server.reload_sender = self.reload_sender.clone();
        Ok(Arc::new(server))
    }

    fn _reload_with_new_notifier(
        &self,
        config: AnyServerConfig,
        _registry: &mut ServerRegistry,
    ) -> anyhow::Result<ArcServerInternal> {
        let server = self.prepare_reload(config)?;
        Ok(Arc::new(server))
    }

    fn _start_runtime(&self, server: ArcServer) -> anyhow::Result<()> {
        let runtime = ListenUdpRuntime::new(
            WrapArcServer(server),
            self.config.listen.clone(),
            vey_types::net::UdpConnectionTrackConfig::default(),
        );
        runtime
            .run_all_instances(self.config.listen_in_worker, &self.reload_sender)
            .map(|_| self.server_stats.set_online())
    }

    fn _abort_runtime(&self) {
        let _ = self.reload_sender.send(ServerReloadCommand::QuitRuntime);
        self.server_stats.set_offline();
    }
}

impl BaseServer for UdpTProxyServer {
    fn name(&self) -> &NodeName {
        self.config.name()
    }

    fn r#type(&self) -> &'static str {
        self.config.r#type()
    }

    fn version(&self) -> usize {
        self.reload_version
    }
}

#[async_trait]
impl AcceptTcpServer for UdpTProxyServer {
    async fn run_tcp_task(&self, _stream: TcpStream, _cc_info: ClientConnectionInfo) {}
}

#[async_trait]
impl AcceptUdpServer for UdpTProxyServer {
    async fn run_udp_task(
        &self,
        cc_info: ClientConnectionInfo,
        packet_receiver: AcceptedUdpPacketReceiver,
        packet_sender: AcceptedUdpPacketSender,
    ) {
        let client_addr = cc_info.client_addr();
        self.server_stats.add_conn(client_addr);
        if self.drop_early(client_addr) {
            return;
        }

        let Some(task_notes) = self.build_task_notes(&cc_info) else {
            return;
        };

        let ctx = CommonTaskContext {
            server_config: self.config.clone(),
            server_stats: self.server_stats.clone(),
            server_quit_policy: self.quit_policy.clone(),
            idle_wheel: self.idle_wheel.clone(),
            escaper: self.escaper.load().as_ref().clone(),
            cc_info,
            task_logger: self.task_logger.clone(),
        };
        UdpTProxyTask::new(ctx, task_notes).into_running(packet_receiver, packet_sender);
    }
}

#[async_trait]
impl AcceptQuicServer for UdpTProxyServer {
    #[cfg(feature = "quic")]
    async fn run_quic_task(&self, _connection: Connection, _cc_info: ClientConnectionInfo) {}
}

#[async_trait]
impl Server for UdpTProxyServer {
    fn escaper(&self) -> &NodeName {
        self.config.escaper()
    }

    fn user_group(&self) -> &NodeName {
        self.config.user_group()
    }

    fn auditor(&self) -> &NodeName {
        self.config.auditor()
    }

    fn get_server_stats(&self) -> Option<ArcServerStats> {
        Some(self.server_stats.clone())
    }

    fn get_listen_stats(&self) -> Arc<ListenStats> {
        Arc::clone(&self.listen_stats)
    }

    fn alive_count(&self) -> i32 {
        self.server_stats.get_alive_count()
    }

    fn quit_policy(&self) -> &Arc<ServerQuitPolicy> {
        &self.quit_policy
    }

    async fn run_rustls_task(&self, _stream: TlsStream<TcpStream>, _cc_info: ClientConnectionInfo) {
    }

    async fn run_openssl_task(
        &self,
        _stream: SslStream<TcpStream>,
        _cc_info: ClientConnectionInfo,
    ) {
    }
}
