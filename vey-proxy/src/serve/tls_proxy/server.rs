/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use arc_swap::{ArcSwap, ArcSwapOption};
use async_trait::async_trait;
use bytes::BytesMut;
use log::debug;
use openssl::ssl::{NameType, Ssl};
#[cfg(feature = "quic")]
use quinn::Connection;
use slog::Logger;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio_rustls::server::TlsStream;

use vey_codec::tls::{
    ClientHello, ExtensionType, HandshakeCoalescer, Record, RecordHeader, RecordParseError,
};
use vey_daemon::listen::{
    AcceptQuicServer, AcceptTcpServer, AcceptUdpServer, AcceptedUdpPacketReceiver,
    AcceptedUdpPacketSender, ListenStats, ListenTcpRuntime,
};
use vey_daemon::server::{BaseServer, ClientConnectionInfo, ServerReloadCommand};
use vey_io_ext::{AsyncStream, IdleWheel, OnceBufReader};
use vey_openssl::{SslAcceptor, SslStream};
use vey_types::acl::{AclAction, AclNetworkRule};
use vey_types::metrics::NodeName;
use vey_types::net::{Host, OpensslTicketKey, RollingTicketer, TlsServerName};
use vey_types::route::HostMatch;

use super::common::CommonTaskContext;
use super::host::TlsHost;
use super::task::TlsProxyTask;
use crate::audit::{AuditContext, AuditHandle};
use crate::config::server::tls_proxy::TlsProxyServerConfig;
use crate::config::server::{AnyServerConfig, ServerConfig};
use crate::escape::ArcEscaper;
use crate::serve::tcp_stream::TcpStreamServerStats;
use crate::serve::{
    ArcServer, ArcServerInternal, ArcServerStats, Server, ServerInternal, ServerQuitPolicy,
    ServerRegistry, ServerStats, ServerTaskNotes, WrapArcServer,
};
use crate::site::SiteContext;

pub(crate) struct TlsProxyServer {
    config: Arc<TlsProxyServerConfig>,
    server_stats: Arc<TcpStreamServerStats>,
    listen_stats: Arc<ListenStats>,
    tls_rolling_ticketer: Option<Arc<RollingTicketer<OpensslTicketKey>>>,
    ingress_net_filter: Option<AclNetworkRule>,
    reload_sender: broadcast::Sender<ServerReloadCommand<()>>,
    task_logger: Option<Logger>,
    hosts: ArcSwap<HostMatch<Arc<TlsHost>>>,

    escaper: ArcSwap<ArcEscaper>,
    audit_handle: ArcSwapOption<AuditHandle>,
    quit_policy: Arc<ServerQuitPolicy>,
    idle_wheel: Arc<IdleWheel>,
    reload_version: usize,
}

impl TlsProxyServer {
    fn new(
        config: Arc<TlsProxyServerConfig>,
        server_stats: Arc<TcpStreamServerStats>,
        listen_stats: Arc<ListenStats>,
        hosts: HostMatch<Arc<TlsHost>>,
        tls_rolling_ticketer: Option<Arc<RollingTicketer<OpensslTicketKey>>>,
        version: usize,
    ) -> anyhow::Result<Self> {
        let reload_sender = ServerReloadCommand::new_sender();

        let ingress_net_filter = config
            .ingress_net_filter
            .as_ref()
            .map(|builder| builder.build());

        let task_logger = config.get_task_logger();
        let idle_wheel = IdleWheel::spawn(config.task_idle_check_interval);

        server_stats.set_extra_tags(config.extra_metrics_tags.clone());

        let escaper = Arc::new(crate::escape::get_or_insert_default(config.escaper()));
        let audit_handle = config.get_audit_handle()?;

        Ok(TlsProxyServer {
            config,
            server_stats,
            listen_stats,
            tls_rolling_ticketer,
            ingress_net_filter,
            reload_sender,
            task_logger,
            hosts: ArcSwap::from_pointee(hosts),
            escaper: ArcSwap::new(escaper),
            audit_handle: ArcSwapOption::new(audit_handle),
            quit_policy: Arc::new(ServerQuitPolicy::default()),
            idle_wheel,
            reload_version: version,
        })
    }

    pub(crate) fn prepare_initial(
        config: TlsProxyServerConfig,
    ) -> anyhow::Result<ArcServerInternal> {
        let config = Arc::new(config);
        let server_stats = Arc::new(TcpStreamServerStats::new(config.name()));
        let listen_stats = Arc::new(ListenStats::new(config.name()));

        let tls_rolling_ticketer = if let Some(c) = &config.tls_ticketer {
            let ticketer = c
                .build_and_spawn_updater()
                .context("failed to create tls rolling ticketer")?;
            Some(ticketer)
        } else {
            None
        };
        let hosts = build_hosts(&config.site_group, tls_rolling_ticketer.clone())?;

        let server = TlsProxyServer::new(
            config,
            server_stats,
            listen_stats,
            hosts,
            tls_rolling_ticketer,
            1,
        )?;
        Ok(Arc::new(server))
    }

    fn prepare_reload(&self, config: AnyServerConfig) -> anyhow::Result<TlsProxyServer> {
        if let AnyServerConfig::TlsProxy(config) = config {
            let config = Arc::new(config);
            let server_stats = Arc::clone(&self.server_stats);
            let listen_stats = Arc::clone(&self.listen_stats);

            let tls_rolling_ticketer = if self.config.tls_ticketer.eq(&config.tls_ticketer) {
                self.tls_rolling_ticketer.clone()
            } else if let Some(c) = &config.tls_ticketer {
                let ticketer = c
                    .build_and_spawn_updater()
                    .context("failed to create tls rolling ticketer")?;
                Some(ticketer)
            } else {
                None
            };
            let hosts = build_hosts(&config.site_group, tls_rolling_ticketer.clone())?;

            let server = TlsProxyServer::new(
                config,
                server_stats,
                listen_stats,
                hosts,
                tls_rolling_ticketer,
                self.reload_version + 1,
            )?;
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

    fn audit_context(&self) -> AuditContext {
        AuditContext::new(self.audit_handle.load_full())
    }

    fn get_common_task_context(&self, cc_info: ClientConnectionInfo) -> CommonTaskContext {
        CommonTaskContext {
            server_config: self.config.clone(),
            server_stats: self.server_stats.clone(),
            server_quit_policy: self.quit_policy.clone(),
            idle_wheel: self.idle_wheel.clone(),
            escaper: self.escaper.load().as_ref().clone(),
            cc_info,
            task_logger: self.task_logger.clone(),
        }
    }

    async fn run_task<S>(&self, stream: S, cc_info: ClientConnectionInfo, host: Arc<TlsHost>)
    where
        S: AsyncStream + 'static,
        S::R: AsyncRead + Send + Sync + Unpin + 'static,
        S::W: AsyncWrite + Send + Sync + Unpin + 'static,
    {
        let site_ctx = SiteContext::new(
            Arc::clone(host.site()),
            Arc::clone(host.egress()),
            self.config.name(),
            self.server_stats.share_extra_tags(),
        );
        let task_notes =
            ServerTaskNotes::new(cc_info.clone(), None, Duration::ZERO).with_site_ctx(site_ctx);

        let ctx = self.get_common_task_context(cc_info);
        TlsProxyTask::new(ctx, host, self.audit_context(), task_notes)
            .into_running(stream)
            .await;
    }

    async fn run_tls_tcp_task(&self, mut stream: TcpStream, cc_info: ClientConnectionInfo) {
        const TLS_MAX_CLIENT_HELLO_SIZE: u32 = 1 << 16;

        let hosts = self.hosts.load();
        let mut clt_r_buf = BytesMut::with_capacity(2048);
        let host = match tokio::time::timeout(
            self.config.client_hello_recv_timeout,
            read_sni_host(
                &mut stream,
                &mut clt_r_buf,
                TLS_MAX_CLIENT_HELLO_SIZE,
                &hosts,
            ),
        )
        .await
        {
            Ok(Ok(host)) => host,
            Ok(Err(e)) => {
                self.listen_stats.add_failed();
                debug!(
                    "{} - {} tls client hello error: {e:?}",
                    cc_info.sock_local_addr(),
                    cc_info.sock_peer_addr()
                );
                return;
            }
            Err(_) => {
                self.listen_stats.add_timeout();
                debug!(
                    "{} - {} tls client hello timeout",
                    cc_info.sock_local_addr(),
                    cc_info.sock_peer_addr()
                );
                return;
            }
        };

        let Some(host) = host.cloned() else {
            self.listen_stats.add_failed();
            debug!(
                "{} - {} tls error: no matched site",
                cc_info.sock_local_addr(),
                cc_info.sock_peer_addr()
            );
            return;
        };

        let tls_config = host.tls_server();
        let Ok(ssl) = Ssl::new(&tls_config.ssl_context) else {
            self.listen_stats.add_failed();
            return;
        };
        let stream = OnceBufReader::new(stream, clt_r_buf);
        let Ok(ssl_acceptor) = SslAcceptor::new(ssl, stream, tls_config.accept_timeout) else {
            self.listen_stats.add_failed();
            return;
        };
        match ssl_acceptor.accept().await {
            Ok(ssl_stream) => {
                if ssl_stream.ssl().session_reused() {
                    cc_info.tcp_sock_try_quick_ack();
                }
                self.run_task(ssl_stream, cc_info, host).await
            }
            Err(e) => {
                self.listen_stats.add_failed();
                debug!(
                    "{} - {} tls error: {e:?}",
                    cc_info.sock_local_addr(),
                    cc_info.sock_peer_addr()
                );
            }
        }
    }

    fn match_sni_host(&self, sni: Option<&str>) -> Option<Arc<TlsHost>> {
        let hosts = self.hosts.load();
        match sni {
            Some(name) => match Host::from_str(name) {
                Ok(host) => hosts.get(&host).cloned(),
                Err(_) => hosts.get_default().cloned(),
            },
            None => hosts.get_default().cloned(),
        }
    }
}

async fn read_sni_host<'a>(
    clt_r: &mut TcpStream,
    clt_r_buf: &mut BytesMut,
    max_client_hello_size: u32,
    hosts: &'a HostMatch<Arc<TlsHost>>,
) -> anyhow::Result<Option<&'a Arc<TlsHost>>> {
    let max_hello_size = max_client_hello_size as usize;
    let max_buf_size = max_hello_size
        .saturating_mul(RecordHeader::SIZE + 1)
        .saturating_add(1 << 14);
    let mut handshake_coalescer = HandshakeCoalescer::new(max_client_hello_size);
    let mut record_offset = 0;
    loop {
        let mut record = match Record::parse(&clt_r_buf[record_offset..]) {
            Ok(r) => r,
            Err(RecordParseError::NeedMoreData(_)) => {
                if clt_r_buf.len() >= max_buf_size {
                    return Err(anyhow!("tls client hello message too large"));
                }
                match clt_r.read_buf(clt_r_buf).await {
                    Ok(0) => return Err(anyhow!("connection closed by client")),
                    Ok(_) => continue,
                    Err(e) => return Err(anyhow!("client read error: {e}")),
                }
            }
            Err(_) => return Err(anyhow!("invalid tls client hello request")),
        };
        record_offset += record.encoded_len();

        match record.consume_handshake(&mut handshake_coalescer) {
            Ok(Some(handshake_msg)) => {
                let ch = handshake_msg
                    .parse_client_hello()
                    .map_err(|_| anyhow!("invalid tls client hello request"))?;
                return Ok(host_from_client_hello(ch, hosts));
            }
            Ok(None) => match handshake_coalescer.parse_client_hello() {
                Ok(Some(ch)) => return Ok(host_from_client_hello(ch, hosts)),
                Ok(None) => {
                    if !record.consume_done() {
                        return Err(anyhow!("partial fragmented tls client hello request"));
                    }
                }
                Err(_) => return Err(anyhow!("invalid fragmented tls client hello request")),
            },
            Err(_) => return Err(anyhow!("invalid tls client hello request")),
        }
    }
}

fn host_from_client_hello<'a>(
    ch: ClientHello<'_>,
    hosts: &'a HostMatch<Arc<TlsHost>>,
) -> Option<&'a Arc<TlsHost>> {
    match ch.get_ext(ExtensionType::ServerName) {
        Ok(Some(data)) => match TlsServerName::from_extension_value(data) {
            Ok(sni) => hosts.get(&Host::from(sni)),
            Err(_) => hosts.get_default(),
        },
        Ok(None) => hosts.get_default(),
        Err(_) => hosts.get_default(),
    }
}

fn build_hosts(
    site_group: &NodeName,
    ticketer: Option<Arc<RollingTicketer<OpensslTicketKey>>>,
) -> anyhow::Result<HostMatch<Arc<TlsHost>>> {
    let group = crate::site::get_or_insert_default(site_group);
    group.config().sites.try_build_arc_filtered(|cfg| {
        let site = group
            .get_site(cfg.id())
            .expect("site group is missing a built site");
        TlsHost::try_build(site, ticketer.clone())
    })
}

impl ServerInternal for TlsProxyServer {
    fn _clone_config(&self) -> AnyServerConfig {
        AnyServerConfig::TlsProxy(self.config.as_ref().clone())
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

    fn _update_user_group_in_place(&self) {}

    fn _site_group(&self) -> &NodeName {
        &self.config.site_group
    }

    fn _update_site_group_in_place(&self) {
        if self.config.site_group.is_empty() {
            return;
        }
        match build_hosts(&self.config.site_group, self.tls_rolling_ticketer.clone()) {
            Ok(hosts) => self.hosts.store(Arc::new(hosts)),
            Err(e) => debug!(
                "failed to rebuild tls_proxy hosts from site group {}: {e:?}",
                self.config.site_group
            ),
        }
    }

    fn _update_audit_handle_in_place(&self) -> anyhow::Result<()> {
        let audit_handle = self.config.get_audit_handle()?;
        self.audit_handle.store(audit_handle);
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
        let Some(listen_config) = &self.config.listen else {
            return Ok(());
        };
        let listen_stats = server.get_listen_stats();
        let mut runtime = ListenTcpRuntime::new(WrapArcServer(server), listen_stats);
        runtime
            .run_all_instances(
                listen_config,
                self.config.listen_in_worker,
                &self.reload_sender,
            )
            .map(|_| self.server_stats.set_online())
    }

    fn _abort_runtime(&self) {
        let _ = self.reload_sender.send(ServerReloadCommand::QuitRuntime);
        self.server_stats.set_offline();
    }
}

impl BaseServer for TlsProxyServer {
    #[inline]
    fn name(&self) -> &NodeName {
        self.config.name()
    }

    #[inline]
    fn r#type(&self) -> &'static str {
        self.config.r#type()
    }

    #[inline]
    fn version(&self) -> usize {
        self.reload_version
    }
}

#[async_trait]
impl AcceptTcpServer for TlsProxyServer {
    async fn run_tcp_task(&self, stream: TcpStream, cc_info: ClientConnectionInfo) {
        let client_addr = cc_info.client_addr();
        self.server_stats.add_conn(client_addr);
        if self.drop_early(client_addr) {
            return;
        }

        self.run_tls_tcp_task(stream, cc_info).await;
    }
}

#[async_trait]
impl AcceptQuicServer for TlsProxyServer {
    #[cfg(feature = "quic")]
    async fn run_quic_task(&self, _connection: Connection, _cc_info: ClientConnectionInfo) {}
}

#[async_trait]
impl AcceptUdpServer for TlsProxyServer {
    async fn run_udp_task(
        &self,
        _cc_info: ClientConnectionInfo,
        _packet_receiver: AcceptedUdpPacketReceiver,
        _packet_sender: AcceptedUdpPacketSender,
    ) {
    }
}

#[async_trait]
impl Server for TlsProxyServer {
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

    #[inline]
    fn quit_policy(&self) -> &Arc<ServerQuitPolicy> {
        &self.quit_policy
    }

    async fn run_rustls_task(&self, stream: TlsStream<TcpStream>, cc_info: ClientConnectionInfo) {
        let client_addr = cc_info.client_addr();
        self.server_stats.add_conn(client_addr);
        if self.drop_early(client_addr) {
            return;
        }

        let sni = stream.get_ref().1.server_name();
        let Some(host) = self.match_sni_host(sni) else {
            self.listen_stats.add_failed();
            return;
        };
        self.run_task(stream, cc_info, host).await;
    }

    async fn run_openssl_task(&self, stream: SslStream<TcpStream>, cc_info: ClientConnectionInfo) {
        let client_addr = cc_info.client_addr();
        self.server_stats.add_conn(client_addr);
        if self.drop_early(client_addr) {
            return;
        }

        let sni = stream.ssl().servername(NameType::HOST_NAME);
        let Some(host) = self.match_sni_host(sni) else {
            self.listen_stats.add_failed();
            return;
        };
        self.run_task(stream, cc_info, host).await;
    }
}
