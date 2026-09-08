/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use arc_swap::{ArcSwap, ArcSwapOption};
use async_trait::async_trait;
use bytes::{Buf, BytesMut};
use log::debug;
use openssl::ssl::Ssl;
#[cfg(feature = "quic")]
use quinn::Connection;
use slog::Logger;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};
use tokio_rustls::server::TlsStream;

use vey_codec::tls::{
    ClientHello, ExtensionType, HandshakeCoalescer, Record, RecordHeader, RecordParseError,
};
use vey_daemon::listen::{
    AcceptQuicServer, AcceptTcpServer, AcceptUdpServer, AcceptedUdpPacketReceiver,
    AcceptedUdpPacketSender, ListenStats, ListenTcpRuntime,
};
use vey_daemon::server::{BaseServer, ClientConnectionInfo, ServerReloadCommand};
use vey_dpi::{
    MaybeProtocol, Protocol, ProtocolInspectError, ProtocolInspectionConfig, ProtocolInspector,
};
use vey_io_ext::{AsyncStream, IdleWheel, OnceBufReader};
use vey_openssl::{SslAcceptor, SslStream};
use vey_types::acl::{AclAction, AclNetworkRule};
use vey_types::metrics::NodeName;
use vey_types::net::{
    AlpnProtocol, Host, OpensslServerConfig, OpensslTicketKey, RollingTicketer, TlsServerName,
};
use vey_types::route::HostMatch;

use super::task::{
    CommonTaskContext, HttpGuardH2ConnectionTask, HttpGuardPipelineReaderTask,
    HttpGuardPipelineStats, HttpGuardPipelineWriterTask,
};
use super::{HttpGuardServerStats, HttpHost};
use crate::audit::AuditHandle;
use crate::config::server::http_guard::HttpGuardServerConfig;
use crate::config::server::{AnyServerConfig, ServerConfig};
use crate::escape::ArcEscaper;
use crate::serve::{
    ArcServer, ArcServerInternal, ArcServerStats, Server, ServerInternal, ServerQuitPolicy,
    ServerRegistry, ServerStats, WrapArcServer,
};
use crate::site::Site;

pub(crate) struct HttpGuardServer {
    config: Arc<HttpGuardServerConfig>,
    server_stats: Arc<HttpGuardServerStats>,
    listen_stats: Arc<ListenStats>,
    tls_rolling_ticketer: Option<Arc<RollingTicketer<OpensslTicketKey>>>,
    global_tls_server: Option<OpensslServerConfig>,
    ingress_net_filter: Option<AclNetworkRule>,
    reload_sender: broadcast::Sender<ServerReloadCommand<()>>,
    task_logger: Option<Logger>,
    http_hosts: ArcSwap<HostMatch<Arc<HttpHost>>>,
    tls_hosts: ArcSwap<HostMatch<Arc<HttpHost>>>,

    escaper: ArcSwap<ArcEscaper>,
    audit_handle: ArcSwapOption<AuditHandle>,
    quit_policy: Arc<ServerQuitPolicy>,
    idle_wheel: Arc<IdleWheel>,
    reload_version: usize,
}

impl HttpGuardServer {
    fn new(
        config: Arc<HttpGuardServerConfig>,
        server_stats: Arc<HttpGuardServerStats>,
        listen_stats: Arc<ListenStats>,
        http_hosts: HostMatch<Arc<HttpHost>>,
        tls_hosts: HostMatch<Arc<HttpHost>>,
        tls_rolling_ticketer: Option<Arc<RollingTicketer<OpensslTicketKey>>>,
        version: usize,
    ) -> anyhow::Result<Self> {
        let reload_sender = ServerReloadCommand::new_sender();

        let global_tls_server = match &config.global_tls_server {
            Some(builder) => {
                let config = builder
                    .build_with_alpn_protocols(
                        Some(vec![
                            AlpnProtocol::Http2,
                            AlpnProtocol::Http11,
                            AlpnProtocol::Http10,
                        ]),
                        tls_rolling_ticketer.clone(),
                    )
                    .context("failed to build global tls server config")?;
                Some(config)
            }
            None => None,
        };

        let ingress_net_filter = config
            .ingress_net_filter
            .as_ref()
            .map(|builder| builder.build());

        let task_logger = config.get_task_logger();
        let idle_wheel = IdleWheel::spawn(config.task_idle_check_interval);

        // always update extra metrics tags
        server_stats.set_extra_tags(config.extra_metrics_tags.clone());

        let escaper = Arc::new(crate::escape::get_or_insert_default(config.escaper()));
        let audit_handle = config.get_audit_handle()?;

        let server = HttpGuardServer {
            config,
            server_stats,
            listen_stats,
            tls_rolling_ticketer,
            global_tls_server,
            ingress_net_filter,
            reload_sender,
            task_logger,
            http_hosts: ArcSwap::from_pointee(http_hosts),
            tls_hosts: ArcSwap::from_pointee(tls_hosts),
            escaper: ArcSwap::new(escaper),
            audit_handle: ArcSwapOption::new(audit_handle),
            quit_policy: Arc::new(ServerQuitPolicy::default()),
            idle_wheel,
            reload_version: version,
        };

        Ok(server)
    }

    pub(crate) fn prepare_initial(
        config: HttpGuardServerConfig,
    ) -> anyhow::Result<ArcServerInternal> {
        let config = Arc::new(config);
        let server_stats = Arc::new(HttpGuardServerStats::new(config.name()));
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

        let server = HttpGuardServer::new(
            config,
            server_stats,
            listen_stats,
            hosts.http,
            hosts.tls,
            tls_rolling_ticketer,
            1,
        )?;
        Ok(Arc::new(server))
    }

    fn prepare_reload(&self, config: AnyServerConfig) -> anyhow::Result<HttpGuardServer> {
        if let AnyServerConfig::HttpGuard(config) = config {
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

            let server = HttpGuardServer::new(
                config,
                server_stats,
                listen_stats,
                hosts.http,
                hosts.tls,
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

    fn get_common_task_context(
        &self,
        cc_info: ClientConnectionInfo,
        pinned_site: Option<Arc<Site>>,
    ) -> Arc<CommonTaskContext> {
        Arc::new(CommonTaskContext {
            server_config: self.config.clone(),
            server_stats: self.server_stats.clone(),
            server_quit_policy: self.quit_policy.clone(),
            idle_wheel: self.idle_wheel.clone(),
            escaper: self.escaper.load().as_ref().clone(),
            cc_info,
            task_logger: self.task_logger.clone(),
            audit_handle: self.audit_handle.load_full(),
            pinned_site,
        })
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

        // TODO add cps limit

        false
    }

    async fn spawn_h1_task<T>(
        &self,
        stream: T,
        cc_info: ClientConnectionInfo,
        hosts: Arc<HostMatch<Arc<HttpHost>>>,
        pinned_site: Option<Arc<Site>>,
    ) where
        T: AsyncStream,
        T::R: AsyncRead + Send + Sync + Unpin + 'static,
        T::W: AsyncWrite + Send + Sync + Unpin + 'static,
    {
        let ctx = self.get_common_task_context(cc_info, pinned_site);
        let pipeline_stats = Arc::new(HttpGuardPipelineStats::default());
        let (task_sender, task_receiver) = mpsc::channel(ctx.server_config.h1.pipeline_size.get());

        // NOTE tls underlying traffic is not counted in (server/task/user) stats

        let (clt_r, clt_w) = stream.into_split();
        let r_task = HttpGuardPipelineReaderTask::new(&ctx, task_sender, clt_r, &pipeline_stats);
        let w_task = HttpGuardPipelineWriterTask::new(&ctx, task_receiver, clt_w);

        tokio::spawn(r_task.into_running());
        w_task.into_running(hosts).await
    }

    async fn spawn_h2_task<T>(
        &self,
        stream: T,
        cc_info: ClientConnectionInfo,
        hosts: Arc<HostMatch<Arc<HttpHost>>>,
        pinned_site: Option<Arc<Site>>,
    ) where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let ctx = self.get_common_task_context(cc_info, pinned_site);
        HttpGuardH2ConnectionTask::new(&ctx, stream, hosts)
            .into_running()
            .await
    }

    async fn spawn_http_task<T>(
        &self,
        stream: T,
        cc_info: ClientConnectionInfo,
        hosts: Arc<HostMatch<Arc<HttpHost>>>,
        alpn: Option<AlpnProtocol>,
        pinned_site: Option<Arc<Site>>,
    ) where
        T: AsyncStream + AsyncRead + AsyncWrite + Unpin + Send + 'static,
        T::R: AsyncRead + Send + Sync + Unpin + 'static,
        T::W: AsyncWrite + Send + Sync + Unpin + 'static,
    {
        if matches!(alpn, Some(AlpnProtocol::Http2)) {
            self.spawn_h2_task(stream, cc_info, hosts, pinned_site)
                .await;
        } else {
            self.spawn_h1_task(stream, cc_info, hosts, pinned_site)
                .await;
        }
    }

    async fn run_tls_tcp_task(
        &self,
        mut stream: TcpStream,
        mut clt_r_buf: BytesMut,
        cc_info: ClientConnectionInfo,
    ) {
        const TLS_MAX_CLIENT_HELLO_SIZE: u32 = 1 << 16;

        let hosts = self.tls_hosts.load();
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

        let Some(tls_config) = host
            .and_then(|h| h.tls_server())
            .or(self.global_tls_server.as_ref())
        else {
            self.listen_stats.add_failed();
            debug!(
                "{} - {} tls error: no matched server config found",
                cc_info.sock_local_addr(),
                cc_info.sock_peer_addr()
            );
            return;
        };

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
                let alpn = ssl_stream
                    .ssl()
                    .selected_alpn_protocol()
                    .and_then(AlpnProtocol::from_selected);
                self.spawn_http_task(
                    ssl_stream,
                    cc_info,
                    self.tls_hosts.load_full(),
                    alpn,
                    host.map(|h| Arc::clone(h.site())),
                )
                .await
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
}

async fn inspect_client_protocol(
    clt_r: &mut TcpStream,
    clt_r_buf: &mut BytesMut,
    server_port: u16,
    inspect_config: &ProtocolInspectionConfig,
) -> anyhow::Result<Protocol> {
    let inspect_buffer_size = inspect_config.data0_buffer_size();
    let mut inspector = ProtocolInspector::default();
    inspector.push_protocol(MaybeProtocol::Http);
    inspector.push_protocol(MaybeProtocol::Ssl);
    loop {
        match inspector.check_client_initial_data(inspect_config, server_port, clt_r_buf.chunk()) {
            Ok(protocol) => return Ok(protocol),
            Err(ProtocolInspectError::NeedMoreData(_)) => {
                if clt_r_buf.len() >= inspect_buffer_size {
                    return Err(anyhow!("unable to detect client protocol"));
                }
                match clt_r.read_buf(clt_r_buf).await {
                    Ok(0) => return Err(anyhow!("connection closed by client")),
                    Ok(_) => {}
                    Err(e) => return Err(anyhow!("client read error: {e}")),
                }
            }
        }
    }
}

async fn read_sni_host<'a>(
    clt_r: &mut TcpStream,
    clt_r_buf: &mut BytesMut,
    max_client_hello_size: u32,
    hosts: &'a HostMatch<Arc<HttpHost>>,
) -> anyhow::Result<Option<&'a Arc<HttpHost>>> {
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
    hosts: &'a HostMatch<Arc<HttpHost>>,
) -> Option<&'a Arc<HttpHost>> {
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
) -> anyhow::Result<HttpGuardHosts> {
    let group = crate::site::get_or_insert_default(site_group);
    let http = group.config().sites.try_build_arc(|cfg| {
        let site = group
            .get_site(cfg.id())
            .expect("site group is missing a built site");
        HttpHost::try_build(site, ticketer.clone())
    })?;
    let tls = http.filter_arc(|host| host.tls_server().is_some());
    Ok(HttpGuardHosts { http, tls })
}

struct HttpGuardHosts {
    http: HostMatch<Arc<HttpHost>>,
    tls: HostMatch<Arc<HttpHost>>,
}

impl ServerInternal for HttpGuardServer {
    fn _clone_config(&self) -> AnyServerConfig {
        AnyServerConfig::HttpGuard(self.config.as_ref().clone())
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

    fn _update_site_group_in_place(&self) -> anyhow::Result<()> {
        if self.config.site_group.is_empty() {
            return Ok(());
        }
        let hosts = build_hosts(&self.config.site_group, self.tls_rolling_ticketer.clone())?;
        self.http_hosts.store(Arc::new(hosts.http));
        self.tls_hosts.store(Arc::new(hosts.tls));
        Ok(())
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

impl BaseServer for HttpGuardServer {
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
impl AcceptTcpServer for HttpGuardServer {
    async fn run_tcp_task(&self, mut stream: TcpStream, cc_info: ClientConnectionInfo) {
        let client_addr = cc_info.client_addr();
        self.server_stats.add_conn(client_addr);
        if self.drop_early(client_addr) {
            return;
        }

        let inspect_config = ProtocolInspectionConfig::default();
        let inspect_buffer_size = inspect_config.data0_buffer_size();
        let mut clt_r_buf = BytesMut::with_capacity(inspect_buffer_size);
        let protocol = match tokio::time::timeout(
            self.config.client_hello_recv_timeout,
            inspect_client_protocol(
                &mut stream,
                &mut clt_r_buf,
                cc_info.server_addr().port(),
                &inspect_config,
            ),
        )
        .await
        {
            Ok(Ok(protocol)) => protocol,
            Ok(Err(e)) => {
                self.listen_stats.add_failed();
                debug!(
                    "{} - {} client protocol error: {e:?}",
                    cc_info.sock_local_addr(),
                    cc_info.sock_peer_addr()
                );
                return;
            }
            Err(_) => {
                self.listen_stats.add_timeout();
                debug!(
                    "{} - {} client protocol timeout",
                    cc_info.sock_local_addr(),
                    cc_info.sock_peer_addr()
                );
                return;
            }
        };

        match protocol {
            Protocol::Http1 => {
                let stream = OnceBufReader::new(stream, clt_r_buf);
                self.spawn_h1_task(stream, cc_info, self.http_hosts.load_full(), None)
                    .await;
            }
            Protocol::Http2 => {
                if !self.config.h2.enable_h2c {
                    self.listen_stats.add_failed();
                    debug!(
                        "{} - {} rejected h2c (disabled)",
                        cc_info.sock_local_addr(),
                        cc_info.sock_peer_addr()
                    );
                    return;
                }
                let stream = OnceBufReader::new(stream, clt_r_buf);
                self.spawn_h2_task(stream, cc_info, self.http_hosts.load_full(), None)
                    .await;
            }
            Protocol::TlsModern | Protocol::TlsTlcp | Protocol::TlsLegacy | Protocol::SslLegacy => {
                self.run_tls_tcp_task(stream, clt_r_buf, cc_info).await;
            }
            other => {
                self.listen_stats.add_failed();
                debug!(
                    "{} - {} rejected client protocol {}",
                    cc_info.sock_local_addr(),
                    cc_info.sock_peer_addr(),
                    other.as_str()
                );
            }
        }
    }
}

#[async_trait]
impl AcceptUdpServer for HttpGuardServer {
    async fn run_udp_task(
        &self,
        _cc_info: ClientConnectionInfo,
        _packet_receiver: AcceptedUdpPacketReceiver,
        _packet_sender: AcceptedUdpPacketSender,
    ) {
    }
}

#[async_trait]
impl AcceptQuicServer for HttpGuardServer {
    #[cfg(feature = "quic")]
    async fn run_quic_task(&self, _connection: Connection, _cc_info: ClientConnectionInfo) {}
}

#[async_trait]
impl Server for HttpGuardServer {
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

        let alpn = stream
            .get_ref()
            .1
            .alpn_protocol()
            .and_then(AlpnProtocol::from_selected);
        let hosts = self.http_hosts.load_full();
        let pinned_site = stream.get_ref().1.server_name().and_then(|sni| {
            hosts
                .get(&Host::from_str(sni).ok()?)
                .map(|h| Arc::clone(h.site()))
        });
        self.spawn_http_task(stream, cc_info, hosts, alpn, pinned_site)
            .await;
    }

    async fn run_openssl_task(&self, stream: SslStream<TcpStream>, cc_info: ClientConnectionInfo) {
        let client_addr = cc_info.client_addr();
        self.server_stats.add_conn(client_addr);
        if self.drop_early(client_addr) {
            return;
        }

        let alpn = stream
            .ssl()
            .selected_alpn_protocol()
            .and_then(AlpnProtocol::from_selected);
        let hosts = self.http_hosts.load_full();
        let pinned_site = stream
            .ssl()
            .servername(openssl::ssl::NameType::HOST_NAME)
            .and_then(|sni| {
                hosts
                    .get(&Host::from_str(sni).ok()?)
                    .map(|h| Arc::clone(h.site()))
            });
        self.spawn_http_task(stream, cc_info, hosts, alpn, pinned_site)
            .await;
    }
}
