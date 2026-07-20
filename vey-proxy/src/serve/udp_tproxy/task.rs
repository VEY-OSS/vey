/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use tokio::time::Instant;

use vey_daemon::listen::{AcceptedUdpPacketReceiver, AcceptedUdpPacketSender};
use vey_daemon::stat::task::UdpConnectTaskStats;
use vey_io_ext::{
    LimitedUdpMoveRecv, LimitedUdpMoveSend, OptionalInterval, UdpCopyRemoteRecv, UdpCopyRemoteSend,
    UdpMoveError, UdpMoveRecv, UdpMoveRemoteReceiver, UdpMoveRemoteSender, UdpMoveSend,
    UdpMoveTransfer,
};
use vey_types::acl::AclAction;
use vey_types::net::UpstreamAddr;

use super::common::CommonTaskContext;
use crate::config::server::ServerConfig;
use crate::escape::EgressNotes;
use crate::log::escape::udp_sendto::EscapeLogForUdpConnectSendTo;
use crate::log::task::udp_connect::TaskLogForUdpConnect;
use crate::module::udp_connect::UdpConnectTaskConf;
use crate::serve::udp_stream::{UdpStreamServerAliveTaskGuard, UdpStreamTaskCltWrapperStats};
use crate::serve::{
    ServerStats, ServerTaskError, ServerTaskForbiddenError, ServerTaskNotes, ServerTaskResult,
    ServerTaskStage,
};

pub(super) struct TProxyStreamTask {
    ctx: CommonTaskContext,
    upstream: UpstreamAddr,
    udp_notes: EgressNotes,
    task_notes: ServerTaskNotes,
    task_stats: Arc<UdpConnectTaskStats>,
    max_idle_count: usize,
    started: bool,
    _alive_guard: Option<UdpStreamServerAliveTaskGuard>,
}

impl Drop for TProxyStreamTask {
    fn drop(&mut self) {
        if self.started {
            self.post_stop();
            self.started = false;
        }
    }
}

impl TProxyStreamTask {
    pub(super) fn new(
        ctx: CommonTaskContext,
        task_notes: ServerTaskNotes,
        upstream: UpstreamAddr,
    ) -> Self {
        let max_idle_count = task_notes
            .user_ctx()
            .and_then(|c| c.user().task_max_idle_count())
            .unwrap_or(ctx.server_config.task_idle_max_count);
        TProxyStreamTask {
            ctx,
            upstream,
            udp_notes: EgressNotes::default(),
            task_notes,
            task_stats: Arc::new(UdpConnectTaskStats::default()),
            max_idle_count,
            started: false,
            _alive_guard: None,
        }
    }

    fn get_log_context(&self) -> Option<TaskLogForUdpConnect<'_>> {
        self.ctx
            .task_logger
            .as_ref()
            .map(|logger| TaskLogForUdpConnect {
                logger,
                task_notes: &self.task_notes,
                tcp_server_addr: None,
                tcp_client_addr: None,
                udp_listen_addr: Some(self.ctx.server_addr()),
                udp_client_addr: Some(self.ctx.client_addr()),
                upstream: Some(&self.upstream),
                udp_notes: &self.udp_notes,
                client_rd_bytes: self.task_stats.clt.recv.get_bytes(),
                client_rd_packets: self.task_stats.clt.recv.get_packets(),
                client_wr_bytes: self.task_stats.clt.send.get_bytes(),
                client_wr_packets: self.task_stats.clt.send.get_packets(),
                remote_rd_bytes: self.task_stats.ups.recv.get_bytes(),
                remote_rd_packets: self.task_stats.ups.recv.get_packets(),
                remote_wr_bytes: self.task_stats.ups.send.get_bytes(),
                remote_wr_packets: self.task_stats.ups.send.get_packets(),
            })
    }

    pub(super) async fn into_running(
        mut self,
        packet_receiver: AcceptedUdpPacketReceiver,
        packet_sender: AcceptedUdpPacketSender,
    ) {
        self.pre_start();
        let e = match self.run(packet_receiver, packet_sender).await {
            Ok(_) => ServerTaskError::ClosedByClient,
            Err(e) => e,
        };
        if let Some(log_ctx) = self.get_log_context() {
            log_ctx.log(e);
        }
    }

    fn pre_start(&mut self) {
        self._alive_guard = Some(self.ctx.server_stats.add_task());

        if let Some(user_ctx) = self.task_notes.user_ctx() {
            user_ctx.foreach_req_stats(|s| {
                s.req_total.add_udp_connect();
                s.req_alive.add_udp_connect();
            });
        }

        if self.ctx.server_config.flush_task_log_on_created
            && let Some(log_ctx) = self.get_log_context()
        {
            log_ctx.log_created();
        }

        self.started = true;
    }

    fn post_stop(&mut self) {
        if let Some(user_ctx) = self.task_notes.user_ctx() {
            user_ctx.foreach_req_stats(|s| s.req_alive.del_udp_connect());

            if let Some(user_req_alive_permit) = self.task_notes.user_req_alive_permit.take() {
                drop(user_req_alive_permit);
            }
        }
    }

    fn handle_user_upstream_acl_action(&self, action: AclAction) -> ServerTaskResult<()> {
        let forbid = match action {
            AclAction::Permit => false,
            AclAction::PermitAndLog => {
                // TODO log permit
                false
            }
            AclAction::Forbid => true,
            AclAction::ForbidAndLog => {
                // TODO log forbid
                true
            }
        };
        if forbid {
            Err(ServerTaskError::ForbiddenByRule(
                ServerTaskForbiddenError::DestDenied,
            ))
        } else {
            Ok(())
        }
    }

    async fn run(
        &mut self,
        clt_r: AcceptedUdpPacketReceiver,
        clt_w: AcceptedUdpPacketSender,
    ) -> ServerTaskResult<()> {
        if let Some(user_ctx) = self.task_notes.user_ctx() {
            let user_ctx = user_ctx.clone();

            if user_ctx.check_rate_limit().is_err() {
                return Err(ServerTaskError::ForbiddenByRule(
                    ServerTaskForbiddenError::RateLimited,
                ));
            }

            match user_ctx.acquire_request_semaphore() {
                Ok(permit) => self.task_notes.user_req_alive_permit = Some(permit),
                Err(_) => {
                    return Err(ServerTaskError::ForbiddenByRule(
                        ServerTaskForbiddenError::FullyLoaded,
                    ));
                }
            }

            let action = user_ctx.check_upstream(&self.upstream);
            self.handle_user_upstream_acl_action(action)?;
        }

        self.task_notes.stage = ServerTaskStage::Connecting;
        let task_conf = UdpConnectTaskConf {
            upstream: &self.upstream,
            relay: self.ctx.server_config.udp_relay,
        };
        let (ups_r, ups_w) = self
            .ctx
            .escaper
            .udp_setup_connection(
                &task_conf,
                &mut self.udp_notes,
                &self.task_notes,
                self.task_stats.clone(),
            )
            .await?;

        self.task_notes.stage = ServerTaskStage::Connected;
        self.run_connected(clt_r, clt_w, ups_r, ups_w).await
    }

    async fn run_connected<UR, UW>(
        &mut self,
        clt_r: AcceptedUdpPacketReceiver,
        clt_w: AcceptedUdpPacketSender,
        ups_r: UR,
        ups_w: UW,
    ) -> ServerTaskResult<()>
    where
        UR: UdpCopyRemoteRecv + Send + Unpin,
        UW: UdpCopyRemoteSend + Send + Unpin,
    {
        if self.ctx.server_config.flush_task_log_on_connected
            && let Some(log_ctx) = self.get_log_context()
        {
            log_ctx.log_connected();
        }
        self.task_notes.mark_relaying();

        self.relay(clt_r, clt_w, ups_r, ups_w).await
    }

    async fn relay<UR, UW>(
        &mut self,
        clt_r: AcceptedUdpPacketReceiver,
        clt_w: AcceptedUdpPacketSender,
        ups_r: UR,
        ups_w: UW,
    ) -> ServerTaskResult<()>
    where
        UR: UdpCopyRemoteRecv + Send + Unpin,
        UW: UdpCopyRemoteSend + Send + Unpin,
    {
        let task_id = &self.task_notes.id;

        let (mut clt_r, mut clt_w) = self.setup_limit_and_stats(clt_r, clt_w);

        let mut ups_r =
            UdpMoveRemoteReceiver::new(ups_r, self.ctx.server_config.udp_relay.packet_size());
        let mut ups_w = UdpMoveRemoteSender::new(ups_w);

        let mut c_to_r =
            UdpMoveTransfer::new(&mut clt_r, &mut ups_w, self.ctx.server_config.udp_relay);
        let mut r_to_c =
            UdpMoveTransfer::new(&mut ups_r, &mut clt_w, self.ctx.server_config.udp_relay);

        let mut idle_interval = self.ctx.idle_wheel.register();
        let mut log_interval = self.get_log_interval();
        let mut idle_count = 0;

        loop {
            tokio::select! {
                biased;

                r = &mut c_to_r => {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(UdpMoveError::RecvError(e)) => {
                            // yes it's send error returned from the udp packet receiver
                            Err(ServerTaskError::ClientUdpSendFailed(e))
                        }
                        Err(UdpMoveError::SendError(e)) => {
                            if let Some(logger) = ups_w.inner().error_logger() {
                                EscapeLogForUdpConnectSendTo {
                                    task_id,
                                    upstream: Some(&self.upstream),
                                    udp_notes: &self.udp_notes,
                                }
                                .log(logger, &e);
                            }
                            clt_w.inner_mut().close().await;
                            Err(e.into())
                        }
                        Err(UdpMoveError::SendZero) => {
                            clt_w.inner_mut().close().await;
                            Err(ServerTaskError::ClosedByUpstream)
                        }
                    };
                }
                r = &mut r_to_c => {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(UdpMoveError::RecvError(e)) => {
                            if let Some(logger) = ups_r.inner().error_logger() {
                                EscapeLogForUdpConnectSendTo {
                                    task_id,
                                    upstream: Some(&self.upstream),
                                    udp_notes: &self.udp_notes,
                                }
                                .log(logger, &e);
                            }
                            clt_w.inner_mut().close().await;
                            Err(e.into())
                        }
                        Err(UdpMoveError::SendError(())) => {
                            Err(ServerTaskError::InternalServerError("the client side packet sender shouldn't return any error"))
                        }
                        Err(UdpMoveError::SendZero) => Err(ServerTaskError::ClosedByClient),
                    };
                }
                _ = log_interval.tick() => {
                    if let Some(log_ctx) = self.get_log_context() {
                        log_ctx.log_periodic();
                    }
                }
                n = idle_interval.tick() => {
                    if c_to_r.is_idle() && r_to_c.is_idle() {
                        idle_count += n;

                        if let Some(user_ctx) = self.task_notes.user_ctx()
                            && user_ctx.user().is_blocked() {
                            clt_w.inner_mut().close().await;
                            return Err(ServerTaskError::CanceledAsUserBlocked);
                        }

                        if idle_count >= self.max_idle_count {
                            clt_w.inner_mut().close().await;
                            return Err(ServerTaskError::Idle(idle_interval.period(), idle_count));
                        }
                    } else {
                        idle_count = 0;
                        c_to_r.reset_active();
                        r_to_c.reset_active();
                    }

                    if self.ctx.server_quit_policy.force_quit() {
                        return Err(ServerTaskError::CanceledAsServerQuit);
                    }
                }
            }
        }
    }

    fn setup_limit_and_stats<CR, CW>(
        &self,
        clt_r: CR,
        clt_w: CW,
    ) -> (LimitedUdpMoveRecv<CR>, LimitedUdpMoveSend<CW>)
    where
        CR: UdpMoveRecv,
        CW: UdpMoveSend,
    {
        let mut wrapper_stats =
            UdpStreamTaskCltWrapperStats::new(&self.ctx.server_stats, &self.task_stats);

        let limit_config = if let Some(user_ctx) = self.task_notes.user_ctx() {
            wrapper_stats.push_user_io_stats(user_ctx.fetch_traffic_stats(
                self.ctx.server_config.name(),
                self.ctx.server_stats.share_extra_tags(),
            ));

            user_ctx
                .user_config()
                .udp_sock_speed_limit
                .shrink_as_smaller(&self.ctx.server_config.udp_sock_speed_limit)
        } else {
            self.ctx.server_config.udp_sock_speed_limit
        };

        let wrapper_stats = Arc::new(wrapper_stats);
        let mut clt_r = LimitedUdpMoveRecv::local_limited(
            clt_r,
            limit_config.shift_millis,
            limit_config.max_north_packets,
            limit_config.max_north_bytes,
            wrapper_stats.clone(),
        );
        let mut clt_w = LimitedUdpMoveSend::local_limited(
            clt_w,
            limit_config.shift_millis,
            limit_config.max_south_packets,
            limit_config.max_south_bytes,
            wrapper_stats,
        );

        if let Some(user_ctx) = self.task_notes.user_ctx() {
            let user = user_ctx.user();
            if let Some(limiter) = user.udp_all_upload_speed_limit() {
                clt_r.add_global_limiter(limiter.clone());
            }
            if let Some(limiter) = user.udp_all_download_speed_limit() {
                clt_w.add_global_limiter(limiter.clone());
            }
        }

        (clt_r, clt_w)
    }

    fn get_log_interval(&self) -> OptionalInterval {
        self.ctx
            .log_flush_interval()
            .map(|log_interval| {
                let interval =
                    tokio::time::interval_at(Instant::now() + log_interval, log_interval);
                OptionalInterval::with(interval)
            })
            .unwrap_or_default()
    }
}
