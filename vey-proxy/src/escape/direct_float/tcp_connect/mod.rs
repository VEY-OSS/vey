/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::borrow::Cow;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use tokio::net::{TcpSocket, TcpStream};
use tokio::task::JoinSet;
use tokio::time::Instant;

use vey_daemon::stat::remote::ArcTcpConnectionTaskRemoteStats;
use vey_io_ext::{LimitedReader, LimitedWriter};
use vey_socket::BindAddr;
use vey_socket::util::AddressFamily;
use vey_types::acl::AclAction;
use vey_types::net::{ConnectError, Host, TcpKeepAliveConfig, UpstreamAddr};

use super::{DirectFloatBindIp, DirectFloatEscaper};
use crate::escape::direct_fixed::tcp_connect::DirectTcpConnectConfig;
use crate::escape::{EgressNotes, EgressSocketType};
use crate::log::escape::tcp_connect::EscapeLogForTcpConnect;
use crate::module::tcp_connect::{
    TcpConnectRemoteWrapperStats, TcpConnectResult, TcpConnectTaskConf, UnderlyingTcpConnectError,
};
use crate::resolve::HappyEyeballsResolveJob;
use crate::serve::ServerTaskNotes;

impl DirectFloatEscaper {
    fn handle_tcp_target_ip_acl_action(
        &self,
        action: AclAction,
        task_notes: &ServerTaskNotes,
    ) -> Result<(), UnderlyingTcpConnectError> {
        let forbid = match action {
            AclAction::Permit => false,
            AclAction::PermitAndLog => {
                // TODO log
                false
            }
            AclAction::Forbid => true,
            AclAction::ForbidAndLog => {
                // TODO log
                true
            }
        };
        if forbid {
            self.stats.forbidden.add_ip_blocked();
            if let Some(user_ctx) = task_notes.user_ctx() {
                user_ctx.add_ip_blocked();
            }
            Err(UnderlyingTcpConnectError::ForbiddenRemoteAddress)
        } else {
            Ok(())
        }
    }

    fn prepare_connect_socket(
        &self,
        peer_ip: IpAddr,
        bind: BindAddr,
        task_notes: &ServerTaskNotes,
        config: &DirectTcpConnectConfig<'_>,
    ) -> Result<(TcpSocket, DirectFloatBindIp), UnderlyingTcpConnectError> {
        match peer_ip {
            IpAddr::V4(_) => {
                if self.config.no_ipv4 {
                    return Err(UnderlyingTcpConnectError::ForbiddenAddressFamily);
                }
            }
            IpAddr::V6(_) => {
                if self.config.no_ipv6 {
                    return Err(UnderlyingTcpConnectError::ForbiddenAddressFamily);
                }
            }
        }

        let (_, action) = self.egress_net_filter.check(peer_ip);
        self.handle_tcp_target_ip_acl_action(action, task_notes)?;

        let bind = if let Some(ip) = bind.ip() {
            self.select_bind_again(ip, task_notes)
                .map_err(UnderlyingTcpConnectError::EscaperNotUsable)?
        } else {
            self.select_bind(AddressFamily::from(&peer_ip), task_notes)
                .map_err(UnderlyingTcpConnectError::EscaperNotUsable)?
        };

        let sock = vey_socket::tcp::new_socket_to(
            peer_ip,
            &BindAddr::Ip(bind.ip),
            &config.keepalive,
            &config.misc_opts,
            true,
        )
        .map_err(UnderlyingTcpConnectError::SetupSocketFailed)?;
        Ok((sock, bind))
    }

    async fn fixed_try_connect(
        &self,
        peer_ip: IpAddr,
        config: DirectTcpConnectConfig<'_>,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<(TcpStream, DirectFloatBindIp), UnderlyingTcpConnectError> {
        let peer = SocketAddr::new(peer_ip, task_conf.upstream.port());
        egress_notes.tcp.peer = Some(peer);

        let (sock, bind) =
            self.prepare_connect_socket(peer_ip, egress_notes.bind, task_notes, &config)?;
        egress_notes.bind = BindAddr::Ip(bind.ip);
        egress_notes.expire = bind.expire_datetime;
        egress_notes.egress = Some(bind.egress_info.clone());

        let instant_now = Instant::now();

        self.stats.tcp.connect.add_attempted();
        egress_notes.tries = 1;
        match tokio::time::timeout(config.connect.each_timeout(), sock.connect(peer)).await {
            Ok(Ok(ups_stream)) => {
                self.stats.tcp.connect.add_success();
                egress_notes.duration = instant_now.elapsed();

                let local_addr = ups_stream
                    .local_addr()
                    .map_err(UnderlyingTcpConnectError::SetupSocketFailed)?;
                self.stats.tcp.connect.add_established();
                egress_notes.tcp.local = Some(local_addr);
                egress_notes.final_addr.target_addr = Some(peer);
                egress_notes.final_addr.outgoing_addr = Some(local_addr);
                Ok((ups_stream, bind))
            }
            Ok(Err(e)) => {
                self.stats.tcp.connect.add_error();
                egress_notes.duration = instant_now.elapsed();

                let e = UnderlyingTcpConnectError::ConnectFailed(ConnectError::from(e));
                if let Some(logger) = &self.escape_logger {
                    EscapeLogForTcpConnect {
                        upstream: task_conf.upstream,
                        egress_notes,
                        task_id: &task_notes.id,
                    }
                    .log(logger, &e);
                }
                Err(e)
            }
            Err(_) => {
                self.stats.tcp.connect.add_timeout();
                egress_notes.duration = instant_now.elapsed();

                let e = UnderlyingTcpConnectError::TimeoutByRule;
                if let Some(logger) = &self.escape_logger {
                    EscapeLogForTcpConnect {
                        upstream: task_conf.upstream,
                        egress_notes,
                        task_id: &task_notes.id,
                    }
                    .log(logger, &e);
                }
                Err(e)
            }
        }
    }

    fn merge_ip_list(&self, tried: usize, ips: &mut Vec<IpAddr>, new: Vec<IpAddr>) {
        self.config.happy_eyeballs.merge_list(tried, ips, new);
    }

    async fn happy_try_connect(
        &self,
        mut resolver_job: HappyEyeballsResolveJob,
        config: DirectTcpConnectConfig<'_>,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<(TcpStream, DirectFloatBindIp), UnderlyingTcpConnectError> {
        let max_tries_each_family = config.connect.max_tries();
        let mut ips = resolver_job
            .get_r1_or_first_many(
                self.config.happy_eyeballs.resolution_delay(),
                max_tries_each_family,
            )
            .await?;

        let mut c_set = JoinSet::new();

        let mut connect_interval =
            tokio::time::interval(self.config.happy_eyeballs.connection_attempt_delay());
        // connect_interval.tick().await; will take 1ms
        // let's use local vars to skip the first tick()
        let mut skip_first_tick = true;

        let mut spawn_new_connection = true;
        let mut running_connection = 0;
        let mut resolver_r2_done = false;
        let each_timeout = config.connect.each_timeout();

        egress_notes.tries = 0;
        let instant_now = Instant::now();
        let mut returned_err = UnderlyingTcpConnectError::NoAddressConnected;

        loop {
            if spawn_new_connection && let Some(ip) = ips.pop() {
                let peer = SocketAddr::new(ip, task_conf.upstream.port());
                egress_notes.tcp.peer = Some(peer);
                let (sock, bind) =
                    self.prepare_connect_socket(ip, egress_notes.bind, task_notes, &config)?;
                running_connection += 1;
                spawn_new_connection = false;
                egress_notes.tries += 1;
                let stats = self.stats.clone();
                c_set.spawn(async move {
                    stats.tcp.connect.add_attempted();
                    match tokio::time::timeout(each_timeout, sock.connect(peer)).await {
                        Ok(Ok(stream)) => {
                            stats.tcp.connect.add_success();
                            (Ok(stream), peer, bind)
                        }
                        Ok(Err(e)) => {
                            stats.tcp.connect.add_error();
                            (
                                Err(UnderlyingTcpConnectError::ConnectFailed(
                                    ConnectError::from(e),
                                )),
                                peer,
                                bind,
                            )
                        }
                        Err(_) => {
                            stats.tcp.connect.add_timeout();
                            (Err(UnderlyingTcpConnectError::TimeoutByRule), peer, bind)
                        }
                    }
                });
                connect_interval.reset();
            }

            if running_connection > 0 {
                tokio::select! {
                    biased;

                    r = c_set.join_next() => {
                        egress_notes.duration = instant_now.elapsed();
                        match r {
                            Some(Ok(r)) => {
                                running_connection -= 1;
                                let peer_addr = r.1;
                                let bind = r.2;
                                egress_notes.tcp.peer = Some(peer_addr);
                                egress_notes.bind = BindAddr::Ip(bind.ip);
                                egress_notes.expire = bind.expire_datetime;
                                egress_notes.egress = Some(bind.egress_info.clone());
                                match r.0 {
                                    Ok(ups_stream) => {
                                        let local_addr = ups_stream
                                            .local_addr()
                                            .map_err(UnderlyingTcpConnectError::SetupSocketFailed)?;
                                        self.stats.tcp.connect.add_established();
                                        egress_notes.tcp.local = Some(local_addr);
                                        egress_notes.final_addr.target_addr = Some(peer_addr);
                                        egress_notes.final_addr.outgoing_addr = Some(local_addr);
                                        return Ok((ups_stream, bind));
                                    }
                                    Err(e) => {
                                        if let Some(logger) = &self.escape_logger {
                                            EscapeLogForTcpConnect {
                                                upstream: task_conf.upstream,
                                                egress_notes,
                                                task_id: &task_notes.id,
                                            }
                                            .log(logger, &e);
                                        }
                                        // TODO tell resolver to remove addr
                                        returned_err = e;
                                        spawn_new_connection = true;
                                    }
                                }
                            }
                            Some(Err(r)) => {
                                running_connection -= 1;
                                if r.is_panic() {
                                    return Err(UnderlyingTcpConnectError::InternalServerError("connect task panic"));
                                }
                                spawn_new_connection = true;
                            }
                            None => unreachable!(),
                        }
                    }
                    _ = connect_interval.tick() => {
                        if skip_first_tick {
                            skip_first_tick = false;
                        } else {
                            spawn_new_connection = true;
                        }
                    }
                    r = resolver_job.get_r2_or_never(max_tries_each_family) => {
                        resolver_r2_done = true;
                        if let Ok(ips2) = r {
                            self.merge_ip_list(egress_notes.tries, &mut ips, ips2);
                        }
                    }
                }
            } else if resolver_r2_done {
                egress_notes.duration = instant_now.elapsed();
                return Err(returned_err);
            } else {
                match tokio::time::timeout(
                    self.config.happy_eyeballs.second_resolution_timeout(),
                    resolver_job.get_r2_or_never(max_tries_each_family),
                )
                .await
                {
                    Ok(Ok(ips2)) => {
                        resolver_r2_done = true;
                        self.merge_ip_list(egress_notes.tries, &mut ips, ips2);
                        spawn_new_connection = true;
                    }
                    Ok(Err(_e)) => {
                        egress_notes.duration = instant_now.elapsed();
                        return Err(returned_err);
                    }
                    Err(_) => {
                        egress_notes.duration = instant_now.elapsed();
                        return Err(UnderlyingTcpConnectError::TimeoutByRule);
                    }
                }
            }
        }
    }

    pub(super) async fn tcp_connect_to(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<(TcpStream, DirectFloatBindIp), UnderlyingTcpConnectError> {
        egress_notes.socket_type = Some(EgressSocketType::Direct);

        let mut config = DirectTcpConnectConfig {
            connect: self.config.general.tcp_connect,
            keepalive: self.config.tcp_keepalive,
            misc_opts: Cow::Borrowed(&self.config.tcp_misc_opts),
        };

        if let Some(user_ctx) = task_notes.user_ctx() {
            let user_config = user_ctx.user_config();

            if let Some(user_config) = &user_config.tcp_connect {
                config.connect.limit_to(user_config);
            }

            config.keepalive = config.keepalive.adjust_to(user_config.tcp_remote_keepalive);
            config.misc_opts = user_config.tcp_remote_misc_opts(&self.config.tcp_misc_opts);
        }

        match task_conf.upstream.host() {
            Host::Ip(ip) => {
                self.fixed_try_connect(*ip, config, task_conf, egress_notes, task_notes)
                    .await
            }
            Host::Domain(domain) => {
                let resolver_job = self.resolve_happy(
                    domain.clone(),
                    self.get_resolve_strategy(task_notes),
                    task_notes,
                )?;

                self.happy_try_connect(resolver_job, config, task_conf, egress_notes, task_notes)
                    .await
            }
        }
    }

    pub(super) async fn tcp_connect_to_again(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        old_upstream: &UpstreamAddr,
        new_egress_notes: &mut EgressNotes,
        old_egress_notes: &EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<(TcpStream, DirectFloatBindIp), UnderlyingTcpConnectError> {
        new_egress_notes.socket_type = Some(EgressSocketType::Direct);
        new_egress_notes.bind = old_egress_notes.bind;

        let mut config = DirectTcpConnectConfig {
            connect: self.config.general.tcp_connect,
            // tcp keepalive is not needed for ftp transfer connection as it shouldn't be idle
            keepalive: TcpKeepAliveConfig::default(),
            misc_opts: Cow::Borrowed(&self.config.tcp_misc_opts),
        };

        if let Some(user_ctx) = task_notes.user_ctx() {
            if let Some(user_config) = &user_ctx.user_config().tcp_connect {
                config.connect.limit_to(user_config);
            }

            config.misc_opts = user_ctx
                .user_config()
                .tcp_remote_misc_opts(&self.config.tcp_misc_opts);
        }

        if task_conf.upstream.host_eq(old_upstream) {
            // This escaper only set tcp.peer for TCP connections
            let control_addr = old_egress_notes.tcp.peer.ok_or_else(|| {
                UnderlyingTcpConnectError::SetupSocketFailed(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "no peer address for referenced connection found",
                ))
            })?;

            self.fixed_try_connect(
                control_addr.ip(),
                config,
                task_conf,
                new_egress_notes,
                task_notes,
            )
            .await
        } else {
            match task_conf.upstream.host() {
                Host::Ip(ip) => {
                    self.fixed_try_connect(*ip, config, task_conf, new_egress_notes, task_notes)
                        .await
                }
                Host::Domain(domain) => {
                    let mut resolve_strategy = self.get_resolve_strategy(task_notes);
                    match new_egress_notes.bind {
                        BindAddr::Ip(IpAddr::V4(_)) => resolve_strategy.query_v4only(),
                        BindAddr::Ip(IpAddr::V6(_)) => resolve_strategy.query_v6only(),
                        #[cfg(target_os = "linux")]
                        BindAddr::Foreign(_) => {
                            return Err(UnderlyingTcpConnectError::InternalServerError(
                                "foreign ip address binding is not supported",
                            ));
                        }
                        _ => {}
                    }

                    let resolver_job =
                        self.resolve_happy(domain.clone(), resolve_strategy, task_notes)?;
                    self.happy_try_connect(
                        resolver_job,
                        config,
                        task_conf,
                        new_egress_notes,
                        task_notes,
                    )
                    .await
                }
            }
        }
    }

    pub(super) async fn tcp_new_connection(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcTcpConnectionTaskRemoteStats,
    ) -> TcpConnectResult {
        let (stream, _) = self
            .tcp_connect_to(task_conf, egress_notes, task_notes)
            .await?;
        let (r, w) = stream.into_split();

        let mut wrapper_stats = TcpConnectRemoteWrapperStats::new(self.stats.clone(), task_stats);
        wrapper_stats.push_user_io_stats(self.fetch_user_upstream_io_stats(task_notes));
        let wrapper_stats = Arc::new(wrapper_stats);

        let limit_config = &self.config.general.tcp_sock_speed_limit;
        let r = LimitedReader::local_limited(
            r,
            limit_config.shift_millis,
            limit_config.max_south,
            wrapper_stats.clone(),
        );
        let w = LimitedWriter::local_limited(
            w,
            limit_config.shift_millis,
            limit_config.max_north,
            wrapper_stats,
        );

        Ok((Box::new(r), Box::new(w)))
    }
}
