/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use std::time::Duration;

use anyhow::anyhow;
use tokio::io::AsyncWriteExt;

use vey_daemon::server::ServerQuitPolicy;
use vey_dpi::ProtocolInspectAction;
use vey_imap_proto::CommandPipeline;
use vey_imap_proto::response::ByeResponse;
use vey_io_ext::{IdleInterval, LineRecvVec, OnceBufReader, StreamCopyConfig};
use vey_slog_types::{LtUpstreamAddr, LtUuid};
use vey_types::net::UpstreamAddr;

use super::StartTlsProtocol;
#[cfg(feature = "quic")]
use crate::audit::DetourAction;
use crate::auth::User;
use crate::config::server::ServerConfig;
use crate::inspect::{
    BoxAsyncRead, BoxAsyncWrite, StreamInspectContext, StreamInspection, StreamTransitTask,
};
use crate::log::task::TaskEvent;
use crate::serve::{ServerTaskError, ServerTaskResult};

mod ext;
use ext::{CommandLineReceiveExt, ResponseLineReceiveExt};

mod capability;
use capability::Capability;

mod greeting;
use greeting::Greeting;

mod not_authenticated;
use not_authenticated::InitiationStatus;

mod authenticated;
use authenticated::CloseReason;

mod forward;
use forward::ResponseAction;

mod logout;

#[cfg(test)]
use authenticated::ClientAction as AuthClientAction;
#[cfg(test)]
use not_authenticated::ClientAction as NaClientAction;

struct ImapRelayBuf {
    rsp_recv_buf: LineRecvVec,
    cmd_recv_buf: LineRecvVec,
}

macro_rules! intercept_log {
    ($obj:tt, $($args:tt)+) => {
        if let Some(logger) = $obj.ctx.intercept_logger() {
            slog::info!(logger, $($args)+;
                "intercept_type" => "ImapConnection",
                "task_id" => LtUuid($obj.ctx.server_task_id()),
                "depth" => $obj.ctx.inspection_depth,
                "upstream" => LtUpstreamAddr(&$obj.upstream),
                "server_bye" => $obj.server_bye,
                "client_logout" => $obj.client_logout,
            );
        }
    };
}

struct ImapIo {
    pub(crate) clt_r: BoxAsyncRead,
    pub(crate) clt_w: BoxAsyncWrite,
    pub(crate) ups_r: OnceBufReader<BoxAsyncRead>,
    pub(crate) ups_w: BoxAsyncWrite,
}

pub(crate) struct ImapInterceptObject<SC: ServerConfig> {
    io: Option<ImapIo>,
    ctx: StreamInspectContext<SC>,
    upstream: UpstreamAddr,
    from_starttls: bool,
    cmd_pipeline: CommandPipeline,
    server_bye: bool,
    client_logout: bool,
    authenticated: bool,
    mailbox_selected: bool,
    capability: Capability,
}

impl<SC: ServerConfig> ImapInterceptObject<SC> {
    pub(crate) fn new(ctx: StreamInspectContext<SC>, upstream: UpstreamAddr) -> Self {
        ImapInterceptObject {
            io: None,
            ctx,
            upstream,
            from_starttls: false,
            cmd_pipeline: CommandPipeline::default(),
            server_bye: false,
            client_logout: false,
            authenticated: false,
            mailbox_selected: false,
            capability: Capability::default(),
        }
    }

    pub(crate) fn set_from_starttls(&mut self) {
        self.from_starttls = true;
    }

    pub(crate) fn set_io(
        &mut self,
        clt_r: BoxAsyncRead,
        clt_w: BoxAsyncWrite,
        ups_r: OnceBufReader<BoxAsyncRead>,
        ups_w: BoxAsyncWrite,
    ) {
        let io = ImapIo {
            clt_r,
            clt_w,
            ups_r,
            ups_w,
        };
        self.io = Some(io);
    }

    fn log_partial_shutdown(&self, task_event: TaskEvent) {
        if let Some(logger) = self.ctx.intercept_logger() {
            slog::info!(logger, "";
                "intercept_type" => "ImapConnection",
                "task_id" => LtUuid(self.ctx.server_task_id()),
                "task_event" => task_event.as_str(),
                "depth" => self.ctx.inspection_depth,
                "upstream" => LtUpstreamAddr(&self.upstream),
            );
        }
    }
}

impl<SC: ServerConfig> StreamTransitTask for ImapInterceptObject<SC> {
    fn copy_config(&self) -> StreamCopyConfig {
        self.ctx.server_config.limited_copy_config()
    }

    fn idle_check_interval(&self) -> IdleInterval {
        self.ctx.idle_wheel.register()
    }

    fn max_idle_count(&self) -> usize {
        self.ctx.max_idle_count
    }

    fn log_client_shutdown(&self) {
        self.log_partial_shutdown(TaskEvent::ClientShutdown);
    }

    fn log_upstream_shutdown(&self) {
        self.log_partial_shutdown(TaskEvent::UpstreamShutdown);
    }

    fn log_periodic(&self) {
        // TODO
    }

    fn log_flush_interval(&self) -> Option<Duration> {
        self.ctx.server_config.task_log_flush_interval()
    }

    fn quit_policy(&self) -> &ServerQuitPolicy {
        self.ctx.server_quit_policy.as_ref()
    }

    fn user(&self) -> Option<&User> {
        self.ctx.user()
    }
}

impl<SC> ImapInterceptObject<SC>
where
    SC: ServerConfig + Send + Sync + 'static,
{
    pub(crate) async fn intercept(mut self) -> ServerTaskResult<Option<StreamInspection<SC>>> {
        let r = match self.ctx.imap_inspect_action(self.upstream.host()) {
            ProtocolInspectAction::Intercept => self.do_intercept().await,
            #[cfg(feature = "quic")]
            ProtocolInspectAction::Detour => self.do_detour().await.map(|_| None),
            ProtocolInspectAction::Bypass => self.do_bypass().await.map(|_| None),
            ProtocolInspectAction::Block => self.do_block().await.map(|_| None),
        };
        match r {
            Ok(obj) => {
                intercept_log!(self, "finished");
                Ok(obj)
            }
            Err(e) => {
                intercept_log!(self, "{e}");
                Err(e)
            }
        }
    }

    #[cfg(feature = "quic")]
    async fn do_detour(&mut self) -> ServerTaskResult<()> {
        let Some(client) = self.ctx.audit_handle.stream_detour_client() else {
            return self.do_bypass().await;
        };

        let mut detour_stream = match client.open_detour_stream().await {
            Ok(s) => s,
            Err(e) => {
                self.close_on_detour_error().await;
                return Err(ServerTaskError::InternalAdapterError(e));
            }
        };

        let detour_ctx = client.build_context(
            &self.ctx.server_config,
            &self.ctx.server_quit_policy,
            &self.ctx.idle_wheel,
            &self.ctx.task_notes,
            &self.upstream,
            vey_dpi::Protocol::Imap,
        );

        match detour_ctx.check_detour_action(&mut detour_stream).await {
            Ok(DetourAction::Continue) => {
                let ImapIo {
                    clt_r,
                    clt_w,
                    ups_r,
                    ups_w,
                } = self.io.take().unwrap();

                detour_ctx
                    .relay(clt_r, clt_w, ups_r, ups_w, detour_stream)
                    .await
            }
            Ok(DetourAction::Bypass) => {
                detour_stream.finish();
                self.do_bypass().await
            }
            Ok(DetourAction::Block) => {
                detour_stream.finish();
                self.do_block().await
            }
            Err(e) => {
                detour_stream.finish();
                self.close_on_detour_error().await;
                Err(ServerTaskError::InternalAdapterError(e))
            }
        }
    }

    #[cfg(feature = "quic")]
    async fn close_on_detour_error(&mut self) {
        let ImapIo {
            clt_r: _,
            mut clt_w,
            ups_r: _,
            mut ups_w,
        } = self.io.take().unwrap();

        tokio::spawn(async move {
            let _ = ups_w.shutdown().await;
        });

        if ByeResponse::reply_internal_error(&mut clt_w).await.is_ok() {
            let _ = clt_w.shutdown().await;
        }
    }

    async fn do_bypass(&mut self) -> ServerTaskResult<()> {
        let ImapIo {
            clt_r,
            clt_w,
            ups_r,
            ups_w,
        } = self.io.take().unwrap();

        self.transit_transparent(clt_r, clt_w, ups_r, ups_w).await
    }

    async fn do_block(&mut self) -> ServerTaskResult<()> {
        let ImapIo {
            clt_r: _,
            mut clt_w,
            ups_r: _,
            mut ups_w,
        } = self.io.take().unwrap();

        tokio::spawn(async move {
            let _ = ups_w.shutdown().await;
        });

        ByeResponse::reply_blocked(&mut clt_w)
            .await
            .map_err(ServerTaskError::ClientTcpWriteFailed)?;
        clt_w
            .shutdown()
            .await
            .map_err(ServerTaskError::ClientTcpWriteFailed)?;
        Err(ServerTaskError::InternalAdapterError(anyhow!(
            "imap blocked by inspection policy"
        )))
    }

    fn mark_close_by_server(&mut self) {
        self.server_bye = true;
    }

    async fn do_intercept(&mut self) -> ServerTaskResult<Option<StreamInspection<SC>>> {
        let ImapIo {
            clt_r,
            mut clt_w,
            ups_r,
            ups_w,
        } = self.io.take().unwrap();

        let interception_config = self.ctx.imap_interception();

        let (initial_data, mut ups_r) = ups_r.into_parts();
        let rsp_recv_buf = if let Some(data) = initial_data {
            LineRecvVec::with_data(&data, interception_config.response_line_max_size)
        } else {
            LineRecvVec::with_capacity(interception_config.response_line_max_size)
        };
        let mut relay_buf = ImapRelayBuf {
            rsp_recv_buf,
            cmd_recv_buf: LineRecvVec::with_capacity(interception_config.command_line_max_size),
        };

        if self.from_starttls {
            return self
                .start_initiation(clt_r, clt_w, ups_r, ups_w, relay_buf)
                .await;
        }

        let mut greeting = Greeting::default();
        if let Err(e) = greeting
            .relay(
                &mut ups_r,
                &mut clt_w,
                &mut relay_buf.rsp_recv_buf,
                interception_config.greeting_timeout,
            )
            .await
        {
            greeting.reply_no_service(&e, &mut clt_w).await;
            return Err(e.into());
        }
        if greeting.close_service() {
            self.mark_close_by_server();
            return Ok(None);
        }
        if greeting.pre_authenticated() {
            self.capability = greeting.into_capability();
            self.enter_authenticated(clt_r, clt_w, ups_r, ups_w, relay_buf)
                .await?;
            Ok(None)
        } else {
            self.capability = greeting.into_capability();
            self.start_initiation(clt_r, clt_w, ups_r, ups_w, relay_buf)
                .await
        }
    }

    async fn start_initiation(
        &mut self,
        mut clt_r: BoxAsyncRead,
        mut clt_w: BoxAsyncWrite,
        mut ups_r: BoxAsyncRead,
        mut ups_w: BoxAsyncWrite,
        mut relay_buf: ImapRelayBuf,
    ) -> ServerTaskResult<Option<StreamInspection<SC>>> {
        match self
            .relay_not_authenticated(
                &mut clt_r,
                &mut clt_w,
                &mut ups_r,
                &mut ups_w,
                &mut relay_buf,
            )
            .await?
        {
            InitiationStatus::ClientClose => {
                self.handle_client_logout(&mut clt_w, &mut ups_r, &mut relay_buf.rsp_recv_buf)
                    .await?;
                Ok(None)
            }
            InitiationStatus::ServerClose => {
                self.mark_close_by_server();
                Ok(None)
            }
            InitiationStatus::LocalClose(e) => {
                self.start_server_logout(&mut ups_r, &mut ups_w, &mut relay_buf.rsp_recv_buf)
                    .await;
                Err(e)
            }
            InitiationStatus::StartTls => {
                if let Some(tls_interception) = self.ctx.tls_interception() {
                    let mut start_tls_obj = crate::inspect::start_tls::StartTlsInterceptObject::new(
                        self.ctx.clone(),
                        self.upstream.clone(),
                        tls_interception,
                        StartTlsProtocol::Imap,
                    );
                    start_tls_obj.set_io(clt_r, clt_w, ups_r, ups_w);
                    Ok(Some(StreamInspection::StartTls(start_tls_obj)))
                } else {
                    self.transit_transparent(clt_r, clt_w, ups_r, ups_w)
                        .await
                        .map(|_| None)
                }
            }
            InitiationStatus::Authenticated => {
                self.enter_authenticated(clt_r, clt_w, ups_r, ups_w, relay_buf)
                    .await?;
                Ok(None)
            }
        }
    }

    async fn enter_authenticated(
        &mut self,
        mut clt_r: BoxAsyncRead,
        mut clt_w: BoxAsyncWrite,
        mut ups_r: BoxAsyncRead,
        mut ups_w: BoxAsyncWrite,
        mut relay_buf: ImapRelayBuf,
    ) -> ServerTaskResult<()> {
        match self
            .relay_authenticated(
                &mut clt_r,
                &mut clt_w,
                &mut ups_r,
                &mut ups_w,
                &mut relay_buf,
            )
            .await?
        {
            CloseReason::Client => {
                self.handle_client_logout(&mut clt_w, &mut ups_r, &mut relay_buf.rsp_recv_buf)
                    .await?;
                let _ = ups_w.shutdown().await;
                let _ = clt_w.shutdown().await;
                Ok(())
            }
            CloseReason::Server => {
                self.mark_close_by_server();
                let _ = ups_w.shutdown().await;
                let _ = clt_w.shutdown().await;
                Ok(())
            }
            CloseReason::Local(e) => {
                self.start_server_logout(&mut ups_r, &mut ups_w, &mut relay_buf.rsp_recv_buf)
                    .await;
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use arc_swap::ArcSwapOption;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use vey_daemon::server::{ClientConnectionInfo, ServerQuitPolicy};
    use vey_io_ext::{IdleWheel, LineRecvVec};
    use vey_types::metrics::{MetricTagMap, NodeName};
    use vey_types::net::UpstreamAddr;
    use vey_types::stats::StatId;

    use super::*;
    use crate::config::server::dummy_close::DummyCloseServerConfig;
    use crate::escape::{EgressNotes, EgressSocketType};
    use crate::inspect::StreamInspectContext;
    use crate::serve::{
        ServerForbiddenSnapshot, ServerForbiddenStats, ServerStats, ServerTaskNotes,
    };

    struct DummyServerStats {
        name: NodeName,
        id: StatId,
        extra: Arc<ArcSwapOption<MetricTagMap>>,
        forbidden: ServerForbiddenStats,
    }

    impl DummyServerStats {
        fn new() -> Self {
            DummyServerStats {
                name: NodeName::default(),
                id: StatId::new_unique(),
                extra: Arc::new(ArcSwapOption::new(None)),
                forbidden: ServerForbiddenStats::default(),
            }
        }
    }

    impl ServerStats for DummyServerStats {
        fn name(&self) -> &NodeName {
            &self.name
        }

        fn stat_id(&self) -> StatId {
            self.id
        }

        fn load_extra_tags(&self) -> Option<Arc<MetricTagMap>> {
            None
        }

        fn share_extra_tags(&self) -> &Arc<ArcSwapOption<MetricTagMap>> {
            &self.extra
        }

        fn is_online(&self) -> bool {
            true
        }

        fn get_conn_total(&self) -> u64 {
            0
        }

        fn get_task_total(&self) -> u64 {
            0
        }

        fn get_alive_count(&self) -> i32 {
            0
        }

        fn forbidden_stats(&self) -> ServerForbiddenSnapshot {
            self.forbidden.snapshot()
        }
    }

    fn test_object() -> ImapInterceptObject<DummyCloseServerConfig> {
        let auditor = crate::audit::get_or_insert_default(&NodeName::default());
        let handle = auditor.build_handle().unwrap();
        let cc_info = ClientConnectionInfo::new(
            "127.0.0.1:12345".parse().unwrap(),
            "127.0.0.1:143".parse().unwrap(),
        );
        let task_notes = ServerTaskNotes::new(cc_info, None, Duration::ZERO);
        let mut egress_notes = EgressNotes {
            socket_type: Some(EgressSocketType::Direct),
            ..Default::default()
        };
        egress_notes.tcp.local = Some("127.0.0.1:12345".parse().unwrap());
        egress_notes.tcp.peer = Some("127.0.0.1:143".parse().unwrap());
        let ctx = StreamInspectContext::new(
            handle,
            Arc::new(DummyCloseServerConfig::new(&NodeName::default(), None)),
            Arc::new(DummyServerStats::new()),
            Arc::new(ServerQuitPolicy::default()),
            IdleWheel::spawn(Duration::from_secs(60)),
            &task_notes,
            &egress_notes,
        );
        ImapInterceptObject::new(
            ctx,
            UpstreamAddr::from_ip_and_port("127.0.0.1".parse().unwrap(), 143),
        )
    }

    fn test_relay_buf() -> ImapRelayBuf {
        ImapRelayBuf {
            rsp_recv_buf: LineRecvVec::with_capacity(4096),
            cmd_recv_buf: LineRecvVec::with_capacity(4096),
        }
    }

    async fn na_cmd(
        obj: &mut ImapInterceptObject<DummyCloseServerConfig>,
        line: &[u8],
    ) -> (NaClientAction, Vec<u8>, Vec<u8>) {
        let mut clt = Vec::new();
        let mut ups = Vec::new();
        let action = obj
            .handle_not_authenticated_cmd_line(line, &mut clt, &mut ups)
            .await
            .unwrap();
        (action, clt, ups)
    }

    async fn auth_cmd(
        obj: &mut ImapInterceptObject<DummyCloseServerConfig>,
        line: &[u8],
    ) -> (AuthClientAction, Vec<u8>, Vec<u8>) {
        let mut clt = Vec::new();
        let mut ups = Vec::new();
        let action = obj
            .handle_authenticated_cmd_line(line, &mut clt, &mut ups)
            .await
            .unwrap();
        (action, clt, ups)
    }

    async fn rsp(
        obj: &mut ImapInterceptObject<DummyCloseServerConfig>,
        line: &[u8],
    ) -> (ResponseAction, Vec<u8>) {
        let mut clt = Vec::new();
        let action = obj.handle_rsp_line(line, &mut clt).await.unwrap();
        (action, clt)
    }

    fn assert_forwarded(clt: &[u8], ups: &[u8], line: &[u8]) {
        assert!(clt.is_empty(), "client {}", String::from_utf8_lossy(clt));
        assert_eq!(ups, line);
    }

    fn assert_client_bad(clt: &[u8], ups: &[u8], tag: &str) {
        assert!(ups.is_empty(), "upstream {}", String::from_utf8_lossy(ups));
        assert_eq!(clt, format!("{tag} BAD invalid command\r\n").as_bytes());
    }

    #[tokio::test]
    async fn not_authenticated_forwards_allowed_commands() {
        let mut obj = test_object();
        for line in [
            &b"A001 CAPABILITY\r\n"[..],
            b"A002 NOOP\r\n",
            b"A003 ID NIL\r\n",
            b"A004 LOGIN user pass\r\n",
            b"A005 AUTHENTICATE PLAIN\r\n",
            b"A006 STARTTLS\r\n",
        ] {
            let (action, clt, ups) = na_cmd(&mut obj, line).await;
            assert!(
                !matches!(action, NaClientAction::Logout),
                "{}",
                String::from_utf8_lossy(line)
            );
            assert_forwarded(&clt, &ups, line);
        }

        let (action, clt, ups) = na_cmd(&mut obj, b"A007 LOGOUT\r\n").await;
        assert!(matches!(action, NaClientAction::Logout));
        assert_forwarded(&clt, &ups, b"A007 LOGOUT\r\n");
    }

    #[tokio::test]
    async fn not_authenticated_rejects_selected_state_commands() {
        let mut obj = test_object();
        for (tag, line) in [
            ("A003", &b"A003 SELECT INBOX\r\n"[..]),
            ("A004", b"A004 FETCH 1:* FLAGS\r\n"),
            ("A005", b"A005 APPEND INBOX {1}\r\n"),
            ("A006", b"A006 ENABLE CONDSTORE\r\n"),
        ] {
            let (action, clt, ups) = na_cmd(&mut obj, line).await;
            assert!(matches!(action, NaClientAction::Loop));
            assert_client_bad(&clt, &ups, tag);
        }
    }

    #[tokio::test]
    async fn login_literals_stay_ongoing_until_complete() {
        let mut obj = test_object();
        let (action, clt, ups) = na_cmd(&mut obj, b"A002 LOGIN {4}\r\n").await;
        assert!(matches!(action, NaClientAction::Loop));
        assert_forwarded(&clt, &ups, b"A002 LOGIN {4}\r\n");
        assert!(obj.cmd_pipeline.ongoing_command().is_some());

        let mut obj = test_object();
        let (action, _, _) = na_cmd(&mut obj, b"A002 LOGIN {4+}\r\n").await;
        assert!(matches!(action, NaClientAction::SendLiteral(4)));
        assert!(obj.cmd_pipeline.ongoing_command().is_some());
    }

    #[tokio::test]
    async fn starttls_rejected_after_already_upgraded() {
        let mut obj = test_object();
        obj.set_from_starttls();
        let (action, clt, ups) = na_cmd(&mut obj, b"A001 STARTTLS\r\n").await;
        assert!(matches!(action, NaClientAction::Loop));
        assert_client_bad(&clt, &ups, "A001");
    }

    #[tokio::test]
    async fn login_ok_sets_authenticated_login_no_does_not() {
        let mut obj = test_object();
        let _ = na_cmd(&mut obj, b"A002 LOGIN user pass\r\n").await;
        let (action, clt) = rsp(&mut obj, b"A002 OK logged in\r\n").await;
        assert!(matches!(action, ResponseAction::Loop));
        assert_eq!(clt, b"A002 OK logged in\r\n");
        assert!(obj.authenticated);

        let mut obj = test_object();
        let _ = na_cmd(&mut obj, b"A002 LOGIN user pass\r\n").await;
        let (action, clt) = rsp(&mut obj, b"A002 NO [AUTHENTICATIONFAILED] failed\r\n").await;
        assert!(matches!(action, ResponseAction::Loop));
        assert_eq!(clt, b"A002 NO [AUTHENTICATIONFAILED] failed\r\n");
        assert!(!obj.authenticated);

        let mut obj = test_object();
        let _ = na_cmd(&mut obj, b"A002 LOGIN user pass\r\n").await;
        let (action, _) = rsp(&mut obj, b"A002 BAD invalid arguments\r\n").await;
        assert!(matches!(action, ResponseAction::Loop));
        assert!(!obj.authenticated);
    }

    #[tokio::test]
    async fn login_ok_after_capability_untagged() {
        let mut obj = test_object();
        let _ = na_cmd(&mut obj, b"A001 CAPABILITY\r\n").await;
        let _ = na_cmd(&mut obj, b"A002 LOGIN user pass\r\n").await;

        let (action, clt) = rsp(
            &mut obj,
            b"* CAPABILITY IMAP4rev1 STARTTLS AUTH=PLAIN COMPRESS=DEFLATE LITERAL+\r\n",
        )
        .await;
        assert!(matches!(action, ResponseAction::Loop));
        assert_eq!(
            clt,
            b"* CAPABILITY IMAP4rev1 STARTTLS AUTH=PLAIN LITERAL+\r\n"
        );
        assert!(!obj.authenticated);

        let (action, _) = rsp(&mut obj, b"A001 OK completed\r\n").await;
        assert!(matches!(action, ResponseAction::Loop));
        assert!(!obj.authenticated);

        let (action, _) = rsp(&mut obj, b"A002 OK logged in\r\n").await;
        assert!(matches!(action, ResponseAction::Loop));
        assert!(obj.authenticated);
    }

    #[tokio::test]
    async fn bye_closes_and_unknown_tag_is_protocol_error() {
        let mut obj = test_object();
        let (action, clt) = rsp(&mut obj, b"* BYE Autologout\r\n").await;
        assert!(matches!(action, ResponseAction::Close));
        assert_eq!(clt, b"* BYE Autologout\r\n");

        let mut obj = test_object();
        let mut clt = Vec::new();
        let err = obj
            .handle_rsp_line(b"Z999 OK unexpected\r\n", &mut clt)
            .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn select_and_close_toggle_mailbox_selected() {
        let mut obj = test_object();
        obj.authenticated = true;
        let _ = auth_cmd(&mut obj, b"A003 SELECT INBOX\r\n").await;
        let (action, _) = rsp(&mut obj, b"A003 OK [READ-WRITE] selected\r\n").await;
        assert!(matches!(action, ResponseAction::Loop));
        assert!(obj.mailbox_selected);

        let _ = auth_cmd(&mut obj, b"A004 CLOSE\r\n").await;
        let _ = rsp(&mut obj, b"A004 OK closed\r\n").await;
        assert!(!obj.mailbox_selected);
    }

    #[tokio::test]
    async fn authenticated_forwards_select_and_rejects_login() {
        let mut obj = test_object();
        obj.authenticated = true;

        let (action, clt, ups) = auth_cmd(&mut obj, b"A003 SELECT INBOX\r\n").await;
        assert!(matches!(action, AuthClientAction::Loop));
        assert_forwarded(&clt, &ups, b"A003 SELECT INBOX\r\n");

        for (tag, line) in [
            ("A004", &b"A004 LOGIN user pass\r\n"[..]),
            ("A005", b"A005 AUTHENTICATE PLAIN\r\n"),
            ("A006", b"A006 STARTTLS\r\n"),
        ] {
            let (action, clt, ups) = auth_cmd(&mut obj, line).await;
            assert!(matches!(action, AuthClientAction::Loop));
            assert_client_bad(&clt, &ups, tag);
        }
    }

    #[tokio::test]
    async fn fetch_requires_selected_mailbox() {
        let mut obj = test_object();
        obj.authenticated = true;
        let (action, clt, ups) = auth_cmd(&mut obj, b"A004 FETCH 1:* FLAGS\r\n").await;
        assert!(matches!(action, AuthClientAction::Loop));
        assert_client_bad(&clt, &ups, "A004");

        obj.mailbox_selected = true;
        let (action, clt, ups) = auth_cmd(&mut obj, b"A005 FETCH 1:* FLAGS\r\n").await;
        assert!(matches!(action, AuthClientAction::Loop));
        assert_forwarded(&clt, &ups, b"A005 FETCH 1:* FLAGS\r\n");
    }

    #[tokio::test]
    async fn enable_without_supported_caps_is_local_ok() {
        let mut obj = test_object();
        obj.authenticated = true;
        let (action, clt, ups) = auth_cmd(&mut obj, b"A001 ENABLE XPIG-LATIN\r\n").await;
        assert!(matches!(action, AuthClientAction::Loop));
        assert!(ups.is_empty());
        assert_eq!(clt, b"A001 OK no enabled\r\n");

        let (action, clt, ups) = auth_cmd(&mut obj, b"A002 ENABLE CONDSTORE\r\n").await;
        assert!(matches!(action, AuthClientAction::Loop));
        assert_forwarded(&clt, &ups, b"A002 ENABLE CONDSTORE\r\n");
    }

    #[tokio::test]
    async fn login_ok_enters_authenticated_and_select_is_forwarded() {
        let mut obj = test_object();
        let mut relay_buf = test_relay_buf();

        let (mut clt_in, mut clt_r) = tokio::io::duplex(4096);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(4096);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(4096);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(4096);

        let login = b"A002 LOGIN user pass\r\n";
        clt_in.write_all(login).await.unwrap();

        let intercept = obj.do_relay_not_authenticated(
            &mut clt_r,
            &mut clt_w,
            &mut ups_r,
            &mut ups_w,
            &mut relay_buf,
        );
        let drive = async {
            let mut buf = [0u8; 64];
            let n = ups_out.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], login);
            ups_in.write_all(b"A002 OK logged in\r\n").await.unwrap();
            let n = clt_out.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], b"A002 OK logged in\r\n");
        };

        let (status, _) = tokio::join!(intercept, drive);
        assert!(matches!(status.unwrap(), InitiationStatus::Authenticated));
        assert!(obj.authenticated);

        let (action, clt, ups) = auth_cmd(&mut obj, b"A003 SELECT INBOX\r\n").await;
        assert!(matches!(action, AuthClientAction::Loop));
        assert_forwarded(&clt, &ups, b"A003 SELECT INBOX\r\n");
    }

    #[tokio::test]
    async fn login_no_stays_not_authenticated() {
        let mut obj = test_object();
        let mut relay_buf = test_relay_buf();

        let (mut clt_in, mut clt_r) = tokio::io::duplex(4096);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(4096);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(4096);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(4096);

        let login = b"A002 LOGIN user pass\r\n";
        let logout = b"A003 LOGOUT\r\n";
        clt_in.write_all(login).await.unwrap();

        let intercept = obj.do_relay_not_authenticated(
            &mut clt_r,
            &mut clt_w,
            &mut ups_r,
            &mut ups_w,
            &mut relay_buf,
        );
        let drive = async {
            let mut buf = [0u8; 64];
            let n = ups_out.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], login);
            ups_in
                .write_all(b"A002 NO [AUTHENTICATIONFAILED] failed\r\n")
                .await
                .unwrap();
            let n = clt_out.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], b"A002 NO [AUTHENTICATIONFAILED] failed\r\n");
            clt_in.write_all(logout).await.unwrap();
            let n = ups_out.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], logout);
        };

        let (status, _) = tokio::join!(intercept, drive);
        assert!(matches!(status.unwrap(), InitiationStatus::ClientClose));
        assert!(!obj.authenticated);
    }

    #[tokio::test]
    async fn authenticate_ok_enters_authenticated() {
        let mut obj = test_object();
        let mut relay_buf = test_relay_buf();

        let (mut clt_in, mut clt_r) = tokio::io::duplex(4096);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(4096);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(4096);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(4096);

        let auth = b"A001 AUTHENTICATE PLAIN\r\n";
        clt_in.write_all(auth).await.unwrap();

        let intercept = obj.do_relay_not_authenticated(
            &mut clt_r,
            &mut clt_w,
            &mut ups_r,
            &mut ups_w,
            &mut relay_buf,
        );
        let drive = async {
            let mut buf = [0u8; 64];
            let n = ups_out.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], auth);
            ups_in.write_all(b"A001 OK logged in\r\n").await.unwrap();
            let n = clt_out.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], b"A001 OK logged in\r\n");
        };

        let (status, _) = tokio::join!(intercept, drive);
        assert!(matches!(status.unwrap(), InitiationStatus::Authenticated));
        assert!(obj.authenticated);
    }

    #[tokio::test]
    async fn starttls_ok_returns_starttls_status() {
        let mut obj = test_object();
        let mut relay_buf = test_relay_buf();

        let (mut clt_in, mut clt_r) = tokio::io::duplex(4096);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(4096);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(4096);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(4096);

        let starttls = b"A001 STARTTLS\r\n";
        clt_in.write_all(starttls).await.unwrap();

        let intercept = obj.do_relay_not_authenticated(
            &mut clt_r,
            &mut clt_w,
            &mut ups_r,
            &mut ups_w,
            &mut relay_buf,
        );
        let drive = async {
            let mut buf = [0u8; 64];
            let n = ups_out.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], starttls);
            ups_in.write_all(b"A001 OK begin TLS\r\n").await.unwrap();
            let n = clt_out.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], b"A001 OK begin TLS\r\n");
        };

        let (status, _) = tokio::join!(intercept, drive);
        assert!(matches!(status.unwrap(), InitiationStatus::StartTls));
        assert!(!obj.authenticated);
    }
}
