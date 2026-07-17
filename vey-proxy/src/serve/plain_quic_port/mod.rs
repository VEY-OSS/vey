/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::{Context, anyhow};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use quinn::Connection;
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio_rustls::server::TlsStream;

use vey_daemon::listen::{
    AcceptQuicServer, AcceptTcpServer, AcceptUdpServer, AcceptedUdpPacketReceiver,
    AcceptedUdpPacketSender, ListenQuicInPlaceConfig, ListenQuicRuntime, ListenStats,
};
use vey_daemon::server::{BaseServer, ClientConnectionInfo, ServerReloadCommand};
use vey_openssl::SslStream;
use vey_types::acl::AclNetworkRule;
use vey_types::metrics::NodeName;
use vey_types::net::{OpensslTicketKey, RollingTicketer};

use crate::config::server::plain_quic_port::{PlainQuicPortConfig, PlainQuicPortUpdateFlags};
use crate::config::server::{AnyServerConfig, ServerConfig};
use crate::serve::{
    ArcServer, ArcServerInternal, Server, ServerInternal, ServerQuitPolicy, ServerRegistry,
    WrapArcServer,
};

pub(crate) struct PlainQuicPort {
    name: NodeName,
    config: ArcSwap<PlainQuicPortConfig>,
    tls_rolling_ticketer: Option<Arc<RollingTicketer<OpensslTicketKey>>>,
    quinn_config: quinn::ServerConfig,
    listen_stats: Arc<ListenStats>,
    ingress_net_filter: Option<Arc<AclNetworkRule>>,
    reload_sender: broadcast::Sender<ServerReloadCommand<ListenQuicInPlaceConfig>>,

    next_server: ArcSwap<ArcServer>,
    quit_policy: Arc<ServerQuitPolicy>,
    reload_version: usize,
}

impl PlainQuicPort {
    fn new<F>(
        config: Arc<PlainQuicPortConfig>,
        listen_stats: Arc<ListenStats>,
        tls_rolling_ticketer: Option<Arc<RollingTicketer<OpensslTicketKey>>>,
        reload_version: usize,
        mut fetch_server: F,
    ) -> anyhow::Result<Self>
    where
        F: FnMut(&NodeName) -> ArcServer,
    {
        let reload_sender = ServerReloadCommand::new_sender();

        let quic_server = config
            .tls_server
            .build_quic_with_ticketer(tls_rolling_ticketer.clone())?;

        let ingress_net_filter = config
            .ingress_net_filter
            .as_ref()
            .map(|builder| Arc::new(builder.build()));

        let next_server = Arc::new(fetch_server(&config.server));

        Ok(PlainQuicPort {
            name: config.name().clone(),
            config: ArcSwap::new(config),
            tls_rolling_ticketer,
            quinn_config: quinn::ServerConfig::with_crypto(quic_server.driver),
            listen_stats,
            ingress_net_filter,
            reload_sender,
            next_server: ArcSwap::new(next_server),
            quit_policy: Arc::new(ServerQuitPolicy::default()),
            reload_version,
        })
    }

    pub(crate) fn prepare_initial(
        config: PlainQuicPortConfig,
    ) -> anyhow::Result<ArcServerInternal> {
        let listen_stats = Arc::new(ListenStats::new(config.name()));

        let tls_rolling_ticketer = if let Some(c) = &config.tls_ticketer {
            let ticketer = c
                .build_and_spawn_updater()
                .context("failed to create tls rolling ticketer")?;
            Some(ticketer)
        } else {
            None
        };

        let server = PlainQuicPort::new(
            Arc::new(config),
            listen_stats,
            tls_rolling_ticketer,
            1,
            crate::serve::get_or_insert_default,
        )?;
        Ok(Arc::new(server))
    }

    fn prepare_reload(
        &self,
        config: AnyServerConfig,
        registry: &mut ServerRegistry,
    ) -> anyhow::Result<PlainQuicPort> {
        if let AnyServerConfig::PlainQuicPort(config) = config {
            let listen_stats = Arc::clone(&self.listen_stats);

            let this_config = self.config.load();
            let tls_rolling_ticketer = if this_config.tls_ticketer.eq(&config.tls_ticketer) {
                self.tls_rolling_ticketer.clone()
            } else if let Some(c) = &config.tls_ticketer {
                let ticketer = c
                    .build_and_spawn_updater()
                    .context("failed to create tls rolling ticketer")?;
                Some(ticketer)
            } else {
                None
            };

            PlainQuicPort::new(
                Arc::new(config),
                listen_stats,
                tls_rolling_ticketer,
                self.reload_version + 1,
                |name| registry.get_or_insert_default(name),
            )
        } else {
            let cur_config = self.config.load();
            Err(anyhow!(
                "config type mismatch: expect {}, actual {}",
                cur_config.r#type(),
                config.r#type()
            ))
        }
    }

    fn update_runtime_in_place(&self, config: ListenQuicInPlaceConfig) -> anyhow::Result<()> {
        self.reload_sender
            .send(ServerReloadCommand::UpdateInPlace(config))
            .map_err(|e| anyhow!("failed to send server reload command: {e}"))?;
        Ok(())
    }
}

impl ServerInternal for PlainQuicPort {
    fn _clone_config(&self) -> AnyServerConfig {
        let config = self.config.load();
        AnyServerConfig::PlainQuicPort(config.as_ref().clone())
    }

    fn _update_config_in_place(&self, flags: u64, config: AnyServerConfig) -> anyhow::Result<()> {
        let AnyServerConfig::PlainQuicPort(config) = config else {
            return Err(anyhow!("invalid config type for PlainQuicPort server"));
        };

        let Some(flags) = PlainQuicPortUpdateFlags::from_bits(flags) else {
            return Err(anyhow!("unknown update flags: {flags}"));
        };

        if flags.contains(PlainQuicPortUpdateFlags::LISTEN_CONFIG) {
            self.update_runtime_in_place(ListenQuicInPlaceConfig::ListenConfig(
                config.listen.clone(),
            ))?;
        }

        if flags.contains(PlainQuicPortUpdateFlags::QUINN_CONFIG) {
            let quic_config = config.tls_server.build_quic()?;
            let quinn_config = quinn::ServerConfig::with_crypto(quic_config.driver);
            self.update_runtime_in_place(ListenQuicInPlaceConfig::QuinnConfig(quinn_config))?;
        }

        if flags.contains(PlainQuicPortUpdateFlags::INGRESS_FILTER) {
            let ingress_net_filter = config
                .ingress_net_filter
                .as_ref()
                .map(|builder| Arc::new(builder.build()));
            self.update_runtime_in_place(ListenQuicInPlaceConfig::IngressAcl(ingress_net_filter))?;
        }

        if flags.contains(PlainQuicPortUpdateFlags::ACCEPT_TIMEOUT) {
            self.update_runtime_in_place(ListenQuicInPlaceConfig::AcceptTimeout(
                config.tls_server.accept_timeout(),
            ))?;
        }

        self.config.store(Arc::new(config));

        if flags.contains(PlainQuicPortUpdateFlags::NEXT_SERVER) {
            self._update_next_servers_in_place();
        }
        Ok(())
    }

    fn _depend_on_server(&self, name: &NodeName) -> bool {
        let config = self.config.load();
        config.server.eq(name)
    }

    fn _reload_config_notify_runtime(&self) {
        let cmd = ServerReloadCommand::ReloadVersion(self.reload_version);
        let _ = self.reload_sender.send(cmd);
    }

    fn _update_next_servers_in_place(&self) {
        let next_server = crate::serve::get_or_insert_default(&self.config.load().server);
        self.next_server.store(Arc::new(next_server));
    }

    fn _update_escaper_in_place(&self) {}
    fn _update_user_group_in_place(&self) {}
    fn _update_audit_handle_in_place(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn _reload_with_old_notifier(
        &self,
        config: AnyServerConfig,
        registry: &mut ServerRegistry,
    ) -> anyhow::Result<ArcServerInternal> {
        let mut server = self.prepare_reload(config, registry)?;
        server.reload_sender = self.reload_sender.clone();
        Ok(Arc::new(server))
    }

    fn _reload_with_new_notifier(
        &self,
        config: AnyServerConfig,
        registry: &mut ServerRegistry,
    ) -> anyhow::Result<ArcServerInternal> {
        let server = self.prepare_reload(config, registry)?;
        Ok(Arc::new(server))
    }

    fn _start_runtime(&self, server: ArcServer) -> anyhow::Result<()> {
        let config = self.config.load();
        let listen_stats = server.get_listen_stats();
        let mut runtime =
            ListenQuicRuntime::new(WrapArcServer(server), listen_stats, config.listen.clone());
        runtime.run_all_instances(
            config.listen_in_worker,
            &self.quinn_config,
            self.ingress_net_filter.as_ref(),
            config.tls_server.accept_timeout(),
            &self.reload_sender,
        )
    }

    fn _abort_runtime(&self) {
        let _ = self.reload_sender.send(ServerReloadCommand::QuitRuntime);
    }
}

impl BaseServer for PlainQuicPort {
    #[inline]
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn r#type(&self) -> &'static str {
        let config = self.config.load();
        config.r#type()
    }

    #[inline]
    fn version(&self) -> usize {
        self.reload_version
    }
}

#[async_trait]
impl AcceptTcpServer for PlainQuicPort {
    async fn run_tcp_task(&self, _stream: TcpStream, _cc_info: ClientConnectionInfo) {}
}

#[async_trait]
impl AcceptUdpServer for PlainQuicPort {
    async fn run_udp_task(
        &self,
        _cc_info: ClientConnectionInfo,
        _packet_receiver: AcceptedUdpPacketReceiver,
        _packet_sender: AcceptedUdpPacketSender,
    ) {
    }
}

#[async_trait]
impl AcceptQuicServer for PlainQuicPort {
    async fn run_quic_task(&self, connection: Connection, cc_info: ClientConnectionInfo) {
        let next_server = self.next_server.load().as_ref().clone();
        next_server.run_quic_task(connection, cc_info).await
    }
}

#[async_trait]
impl Server for PlainQuicPort {
    fn escaper(&self) -> &NodeName {
        Default::default()
    }

    fn user_group(&self) -> &NodeName {
        Default::default()
    }

    fn auditor(&self) -> &NodeName {
        Default::default()
    }

    fn get_listen_stats(&self) -> Arc<ListenStats> {
        Arc::clone(&self.listen_stats)
    }

    fn alive_count(&self) -> i32 {
        0
    }

    #[inline]
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
