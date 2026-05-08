/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use tokio::time::Instant;

use vey_daemon::listen::{AcceptedUdpPacketReceiver, AcceptedUdpPacketSender};
use vey_daemon::stat::task::UdpConnectTaskStats;
use vey_io_ext::{
    OptionalInterval, UdpCopyClientRecv, UdpCopyClientSend, UdpCopyClientToRemote, UdpCopyError,
    UdpCopyRemoteRecv, UdpCopyRemoteSend, UdpCopyRemoteToClient,
};
use vey_types::acl::AclAction;
use vey_types::net::UpstreamAddr;

use super::common::CommonTaskContext;
use super::recv::UdpTProxyClientRecv;
use super::send::UdpTProxyClientSend;
use crate::log::escape::udp_sendto::EscapeLogForUdpConnectSendTo;
use crate::log::task::udp_connect::TaskLogForUdpConnect;
use crate::module::udp_connect::{UdpConnectTaskConf, UdpConnectTaskNotes};
use crate::serve::udp_stream::UdpStreamServerAliveTaskGuard;
use crate::serve::{
    ServerTaskError, ServerTaskForbiddenError, ServerTaskNotes, ServerTaskResult, ServerTaskStage,
};

pub(super) struct UdpTProxyTask {
    ctx: CommonTaskContext,
    upstream: UpstreamAddr,
    udp_notes: UdpConnectTaskNotes,
    task_notes: ServerTaskNotes,
    task_stats: Arc<UdpConnectTaskStats>,
    max_idle_count: usize,
    started: bool,
    _alive_guard: Option<UdpStreamServerAliveTaskGuard>,
}

impl Drop for UdpTProxyTask {
    fn drop(&mut self) {
        if self.started {
            self.post_stop();
        }
    }
}

impl UdpTProxyTask {
    pub(super) fn new(ctx: CommonTaskContext, task_notes: ServerTaskNotes) -> Self {
        let max_idle_count = task_notes
            .user_ctx()
            .and_then(|c| c.user().task_max_idle_count())
            .unwrap_or(ctx.server_config.task_idle_max_count);
        let upstream = UpstreamAddr::from(ctx.target_addr());
        UdpTProxyTask {
            ctx,
            upstream,
            udp_notes: UdpConnectTaskNotes::default(),
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
                tcp_server_addr: self.ctx.server_addr(),
                tcp_client_addr: self.ctx.client_addr(),
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

    pub(super) fn into_running(
        mut self,
        packet_receiver: AcceptedUdpPacketReceiver,
        packet_sender: AcceptedUdpPacketSender,
    ) {
        tokio::spawn(async move {
            self.pre_start();
            let e = match self.run(packet_receiver, packet_sender).await {
                Ok(_) => ServerTaskError::ClosedByClient,
                Err(e) => e,
            };
            if let Some(log_ctx) = self.get_log_context() {
                log_ctx.log(e);
            }
        });
    }

    fn pre_start(&mut self) {
        self._alive_guard = Some(self.ctx.server_stats.add_task());
        if self.ctx.server_config.flush_task_log_on_created
            && let Some(log_ctx) = self.get_log_context()
        {
            log_ctx.log_created();
        }
        self.started = true;
    }

    fn post_stop(&mut self) {
        if let Some(user_req_alive_permit) = self.task_notes.user_req_alive_permit.take() {
            drop(user_req_alive_permit);
        }
    }

    fn handle_user_upstream_acl_action(&self, action: AclAction) -> ServerTaskResult<()> {
        let forbid = match action {
            AclAction::Permit | AclAction::PermitAndLog => false,
            AclAction::Forbid | AclAction::ForbidAndLog => true,
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
        packet_receiver: AcceptedUdpPacketReceiver,
        packet_sender: AcceptedUdpPacketSender,
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

        let clt_r = UdpTProxyClientRecv::new(packet_receiver);
        let clt_w = UdpTProxyClientSend::new(packet_sender);

        self.task_notes.stage = ServerTaskStage::Connecting;
        let task_conf = UdpConnectTaskConf {
            upstream: &self.upstream,
            sock_buf: self.ctx.server_config.udp_socket_buffer,
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
        if self.ctx.server_config.flush_task_log_on_connected
            && let Some(log_ctx) = self.get_log_context()
        {
            log_ctx.log_connected();
        }
        self.task_notes.mark_relaying();

        self.run_relay(Box::new(clt_r), Box::new(clt_w), ups_r, ups_w)
            .await
    }

    async fn run_relay(
        &mut self,
        mut clt_r: Box<dyn UdpCopyClientRecv + Unpin + Send>,
        mut clt_w: Box<dyn UdpCopyClientSend + Unpin + Send>,
        mut ups_r: Box<dyn UdpCopyRemoteRecv + Unpin + Send>,
        mut ups_w: Box<dyn UdpCopyRemoteSend + Unpin + Send>,
    ) -> ServerTaskResult<()> {
        let task_id = &self.task_notes.id;

        let mut c_to_r =
            UdpCopyClientToRemote::new(&mut *clt_r, &mut *ups_w, self.ctx.server_config.udp_relay);
        let mut r_to_c =
            UdpCopyRemoteToClient::new(&mut *clt_w, &mut *ups_r, self.ctx.server_config.udp_relay);

        let mut idle_interval = self.ctx.idle_wheel.register();
        let mut log_interval = self.get_log_interval();
        let mut idle_count = 0;

        loop {
            tokio::select! {
                biased;

                r = &mut c_to_r => {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(UdpCopyError::RemoteError(e)) => {
                            if let Some(logger) = ups_w.error_logger() {
                                EscapeLogForUdpConnectSendTo {
                                    task_id,
                                    upstream: Some(&self.upstream),
                                    udp_notes: &self.udp_notes,
                                }
                                .log(logger, &e);
                            }
                            Err(e.into())
                        }
                        Err(UdpCopyError::ClientError(e)) => Err(e.into()),
                    };
                }
                r = &mut r_to_c => {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(UdpCopyError::RemoteError(e)) => {
                            if let Some(logger) = ups_r.error_logger() {
                                EscapeLogForUdpConnectSendTo {
                                    task_id,
                                    upstream: Some(&self.upstream),
                                    udp_notes: &self.udp_notes,
                                }
                                .log(logger, &e);
                            }
                            Err(e.into())
                        }
                        Err(UdpCopyError::ClientError(e)) => Err(e.into()),
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
                            return Err(ServerTaskError::CanceledAsUserBlocked);
                        }

                        if idle_count >= self.max_idle_count {
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
