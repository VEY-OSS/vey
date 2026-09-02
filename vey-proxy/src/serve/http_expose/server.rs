/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use arc_swap::{ArcSwap, ArcSwapOption};
use async_trait::async_trait;
use bytes::BytesMut;
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
use vey_io_ext::{AsyncStream, IdleWheel, OnceBufReader};
use vey_openssl::{SslAcceptor, SslStream};
use vey_types::acl::{AclAction, AclNetworkRule};
use vey_types::metrics::NodeName;
use vey_types::net::{
    AlpnProtocol, Host, OpensslServerConfig, OpensslTicketKey, RollingTicketer, TlsServerName,
};
use vey_types::route::HostMatch;

use super::task::{
    CommonTaskContext, HttpExposePipelineReaderTask, HttpExposePipelineStats,
    HttpExposePipelineWriterTask,
};
use super::{HttpExposeServerStats, HttpHost};
use crate::auth::UserGroup;
use crate::config::server::http_expose::HttpExposeServerConfig;
use crate::config::server::{AnyServerConfig, ServerConfig};
use crate::escape::ArcEscaper;
use crate::serve::{
    ArcServer, ArcServerInternal, ArcServerStats, Server, ServerInternal, ServerQuitPolicy,
    ServerRegistry, ServerStats, WrapArcServer,
};

pub(crate) struct HttpExposeServer {
    config: Arc<HttpExposeServerConfig>,
    server_stats: Arc<HttpExposeServerStats>,
    listen_stats: Arc<ListenStats>,
    tls_rolling_ticketer: Option<Arc<RollingTicketer<OpensslTicketKey>>>,
    global_tls_server: Option<OpensslServerConfig>,
    ingress_net_filter: Option<AclNetworkRule>,
    reload_sender: broadcast::Sender<ServerReloadCommand<()>>,
    task_logger: Option<Logger>,
    hosts: ArcSwap<HostMatch<Arc<HttpHost>>>,

    escaper: ArcSwap<ArcEscaper>,
    user_group: ArcSwapOption<UserGroup>,
    quit_policy: Arc<ServerQuitPolicy>,
    idle_wheel: Arc<IdleWheel>,
    reload_version: usize,
}

impl HttpExposeServer {
    fn new(
        config: Arc<HttpExposeServerConfig>,
        server_stats: Arc<HttpExposeServerStats>,
        listen_stats: Arc<ListenStats>,
        hosts: HostMatch<Arc<HttpHost>>,
        tls_rolling_ticketer: Option<Arc<RollingTicketer<OpensslTicketKey>>>,
        version: usize,
    ) -> anyhow::Result<Self> {
        let reload_sender = ServerReloadCommand::new_sender();

        let global_tls_server = match &config.global_tls_server {
            Some(builder) => {
                let config = builder
                    .build_with_alpn_protocols(
                        Some(vec![AlpnProtocol::Http11, AlpnProtocol::Http10]),
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
        let user_group = config.get_user_group().map(Arc::new);

        let server = HttpExposeServer {
            config,
            server_stats,
            listen_stats,
            tls_rolling_ticketer,
            global_tls_server,
            ingress_net_filter,
            reload_sender,
            task_logger,
            hosts: ArcSwap::from_pointee(hosts),
            escaper: ArcSwap::new(escaper),
            user_group: ArcSwapOption::new(user_group),
            quit_policy: Arc::new(ServerQuitPolicy::default()),
            idle_wheel,
            reload_version: version,
        };

        Ok(server)
    }

    pub(crate) fn prepare_initial(
        config: HttpExposeServerConfig,
    ) -> anyhow::Result<ArcServerInternal> {
        let config = Arc::new(config);
        let server_stats = Arc::new(HttpExposeServerStats::new(config.name()));
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

        let server = HttpExposeServer::new(
            config,
            server_stats,
            listen_stats,
            hosts,
            tls_rolling_ticketer,
            1,
        )?;
        Ok(Arc::new(server))
    }

    fn prepare_reload(&self, config: AnyServerConfig) -> anyhow::Result<HttpExposeServer> {
        if let AnyServerConfig::HttpExpose(config) = config {
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

            let server = HttpExposeServer::new(
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

    fn get_common_task_context(&self, cc_info: ClientConnectionInfo) -> Arc<CommonTaskContext> {
        Arc::new(CommonTaskContext {
            server_config: self.config.clone(),
            server_stats: self.server_stats.clone(),
            server_quit_policy: self.quit_policy.clone(),
            idle_wheel: self.idle_wheel.clone(),
            escaper: self.escaper.load().as_ref().clone(),
            cc_info,
            task_logger: self.task_logger.clone(),
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

    async fn spawn_stream_task<T>(&self, stream: T, cc_info: ClientConnectionInfo)
    where
        T: AsyncStream,
        T::R: AsyncRead + Send + Sync + Unpin + 'static,
        T::W: AsyncWrite + Send + Sync + Unpin + 'static,
    {
        let ctx = self.get_common_task_context(cc_info);
        let pipeline_stats = Arc::new(HttpExposePipelineStats::default());
        let (task_sender, task_receiver) = mpsc::channel(ctx.server_config.pipeline_size.get());

        // NOTE tls underlying traffic is not counted in (server/task/user) stats

        let (clt_r, clt_w) = stream.into_split();
        let r_task = HttpExposePipelineReaderTask::new(&ctx, task_sender, clt_r, &pipeline_stats);
        let w_task = HttpExposePipelineWriterTask::new(
            &ctx,
            self.user_group.load_full(),
            task_receiver,
            clt_w,
            &pipeline_stats,
        );

        tokio::spawn(r_task.into_running());
        w_task.into_running(self.hosts.load_full()).await
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
                self.spawn_stream_task(ssl_stream, cc_info).await
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
) -> anyhow::Result<HostMatch<Arc<HttpHost>>> {
    let group = crate::site::get_or_insert_default(site_group);
    group.config().sites.try_build_arc(|cfg| {
        let site = group
            .get_site(cfg.id())
            .expect("site group is missing a built site");
        HttpHost::try_build(site, ticketer.clone())
    })
}

impl ServerInternal for HttpExposeServer {
    fn _clone_config(&self) -> AnyServerConfig {
        AnyServerConfig::HttpExpose(self.config.as_ref().clone())
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
        self.user_group
            .store(self.config.get_user_group().map(Arc::new));
    }

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
                "failed to rebuild http_expose hosts from site group {}: {e:?}",
                self.config.site_group
            ),
        }
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

impl BaseServer for HttpExposeServer {
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
impl AcceptTcpServer for HttpExposeServer {
    async fn run_tcp_task(&self, stream: TcpStream, cc_info: ClientConnectionInfo) {
        let client_addr = cc_info.client_addr();
        self.server_stats.add_conn(client_addr);
        if self.drop_early(client_addr) {
            return;
        }

        if self.config.enable_tls_server {
            self.run_tls_tcp_task(stream, cc_info).await;
        } else {
            self.spawn_stream_task(stream, cc_info).await;
        }
    }
}

#[async_trait]
impl AcceptUdpServer for HttpExposeServer {
    async fn run_udp_task(
        &self,
        _cc_info: ClientConnectionInfo,
        _packet_receiver: AcceptedUdpPacketReceiver,
        _packet_sender: AcceptedUdpPacketSender,
    ) {
    }
}

#[async_trait]
impl AcceptQuicServer for HttpExposeServer {
    #[cfg(feature = "quic")]
    async fn run_quic_task(&self, _connection: Connection, _cc_info: ClientConnectionInfo) {}
}

#[async_trait]
impl Server for HttpExposeServer {
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

        self.spawn_stream_task(stream, cc_info).await;
    }

    async fn run_openssl_task(&self, stream: SslStream<TcpStream>, cc_info: ClientConnectionInfo) {
        let client_addr = cc_info.client_addr();
        self.server_stats.add_conn(client_addr);
        if self.drop_early(client_addr) {
            return;
        }

        self.spawn_stream_task(stream, cc_info).await;
    }
}
