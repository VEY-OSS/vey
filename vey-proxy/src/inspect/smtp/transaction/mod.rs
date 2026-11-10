/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::IpAddr;
use std::time::Duration;

use anyhow::anyhow;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::Instant;

use vey_dpi::SmtpInterceptionConfig;
use vey_icap_client::reqmod::mail::{ReqmodAdaptationEndState, ReqmodAdaptationRunState};
use vey_icap_client::reqmod::smtp::SmtpMessageAdapter;
use vey_io_ext::{LimitedWriteExt, StreamCopy, StreamCopyError};
use vey_slog_types::LtUuid;
use vey_smtp_proto::command::{Command, MailParam, RecipientParam};
use vey_smtp_proto::io::TextDataReader;
use vey_smtp_proto::response::{ReplyCode, ResponseEncoder, ResponseParser};

use super::{CommandLineRecvExt, ResponseLineRecvExt, ResponseParseExt, SmtpRelayBuf};
use crate::config::server::ServerConfig;
use crate::inspect::StreamInspectContext;
use crate::serve::{ServerIdleChecker, ServerTaskError, ServerTaskResult};

macro_rules! intercept_log {
    ($obj:tt, $($args:tt)+) => {
        if let Some(logger) = $obj.ctx.intercept_logger() {
            slog::info!(logger, $($args)+;
                "intercept_type" => "SmtpTransaction",
                "task_id" => LtUuid($obj.ctx.server_task_id()),
                "depth" => $obj.ctx.inspection_depth,
                "transaction_id" => $obj.transaction_id,
                "mail_from" => $obj.mail_from.reverse_path(),
            );
        }
    };
}

pub(super) struct Transaction<'a, SC: ServerConfig> {
    config: &'a SmtpInterceptionConfig,
    ctx: &'a StreamInspectContext<SC>,
    transaction_id: usize,
    local_ip: IpAddr,
    allow_chunking: bool,
    allow_burl: bool,
    mail_from: MailParam,
    mail_to: Vec<RecipientParam>,
    quit: bool,
}

impl<'a, SC: ServerConfig> Transaction<'a, SC> {
    pub(super) fn new(
        ctx: &'a StreamInspectContext<SC>,
        transaction_id: usize,
        local_ip: IpAddr,
        allow_chunking: bool,
        allow_burl: bool,
        from: MailParam,
    ) -> Self {
        Transaction {
            config: ctx.smtp_interception(),
            ctx,
            transaction_id,
            local_ip,
            allow_chunking,
            allow_burl,
            mail_from: from,
            mail_to: Vec::with_capacity(4),
            quit: false,
        }
    }

    #[inline]
    pub(super) fn quit(&self) -> bool {
        self.quit
    }

    pub(super) async fn relay<CR, CW, UR, UW>(
        &mut self,
        buf: &mut SmtpRelayBuf,
        clt_r: &mut CR,
        clt_w: &mut CW,
        ups_r: &mut UR,
        ups_w: &mut UW,
    ) -> ServerTaskResult<()>
    where
        CR: AsyncRead + Unpin,
        CW: AsyncWrite + Unpin,
        UR: AsyncRead + Unpin,
        UW: AsyncWrite + Unpin,
    {
        match self.do_relay(buf, clt_r, clt_w, ups_r, ups_w).await {
            Ok(_) => {
                intercept_log!(self, "finished");
                Ok(())
            }
            Err(e) => {
                intercept_log!(self, "{e}");
                Err(e)
            }
        }
    }

    async fn do_relay<CR, CW, UR, UW>(
        &mut self,
        buf: &mut SmtpRelayBuf,
        clt_r: &mut CR,
        clt_w: &mut CW,
        ups_r: &mut UR,
        ups_w: &mut UW,
    ) -> ServerTaskResult<()>
    where
        CR: AsyncRead + Unpin,
        CW: AsyncWrite + Unpin,
        UR: AsyncRead + Unpin,
        UW: AsyncWrite + Unpin,
    {
        let mut in_chunking = false;
        loop {
            buf.cmd_recv_buf.consume_line();
            let (cmd, cmd_line) = buf
                .cmd_recv_buf
                .recv_cmd(self.config.command_wait_timeout, clt_r, clt_w)
                .await?;

            match cmd {
                Command::Recipient(p) => {
                    if in_chunking {
                        self.send_error_to_client(clt_w, ResponseEncoder::BAD_SEQUENCE_OF_COMMANDS)
                            .await?;
                        continue;
                    }
                    self.send_cmd(ups_w, clt_w, cmd_line).await?;
                    let _ = self
                        .recv_relay_rsp(self.config.response_wait_timeout, buf, ups_r, clt_w)
                        .await?;
                    self.mail_to.push(p);
                }
                Command::Data => {
                    if in_chunking {
                        self.send_error_to_client(clt_w, ResponseEncoder::BAD_SEQUENCE_OF_COMMANDS)
                            .await?;
                        continue;
                    }
                    self.send_data_cmd(ups_w, clt_w, cmd_line).await?;
                    let rsp = self
                        .recv_relay_rsp(self.config.data_initiation_timeout, buf, ups_r, clt_w)
                        .await?;
                    if rsp != ReplyCode::START_MAIL_INPUT {
                        continue;
                    }
                    self.send_txt_data(clt_r, clt_w, ups_w).await?;
                    let _ = self
                        .recv_relay_rsp(self.config.data_termination_timeout, buf, ups_r, clt_w)
                        .await?;
                    return Ok(());
                }
                Command::BinaryData(size) => {
                    if !self.allow_chunking {
                        self.send_error_to_client(clt_w, ResponseEncoder::COMMAND_NOT_IMPLEMENTED)
                            .await?;
                        self.discard_bin_data(buf, clt_r, size).await?;
                        continue;
                    }
                    self.send_bdat_cmd(ups_w, clt_w, cmd_line, size).await?;
                    self.send_bin_data(buf, clt_r, ups_w, size).await?;
                    let _ = self
                        .recv_relay_rsp(self.config.data_termination_timeout, buf, ups_r, clt_w)
                        .await?;
                    in_chunking = true;
                }
                Command::LastBinaryData(size) => {
                    if !self.allow_chunking {
                        self.send_error_to_client(clt_w, ResponseEncoder::COMMAND_NOT_IMPLEMENTED)
                            .await?;
                        self.discard_bin_data(buf, clt_r, size).await?;
                        continue;
                    }
                    self.send_bdat_cmd(ups_w, clt_w, cmd_line, size).await?;
                    self.send_bin_data(buf, clt_r, ups_w, size).await?;
                    let _ = self
                        .recv_relay_rsp(self.config.data_termination_timeout, buf, ups_r, clt_w)
                        .await?;
                    return Ok(());
                }
                Command::DataByUrl(url) => {
                    if !self.allow_burl || !self.allow_chunking {
                        self.send_error_to_client(clt_w, ResponseEncoder::COMMAND_NOT_IMPLEMENTED)
                            .await?;
                        continue;
                    }
                    self.send_burl_cmd(ups_w, clt_w, cmd_line, url).await?;
                    let _ = self
                        .recv_relay_rsp(self.config.data_termination_timeout, buf, ups_r, clt_w)
                        .await?;
                    in_chunking = true;
                }
                Command::LastDataByUrl(url) => {
                    if !self.allow_burl {
                        self.send_error_to_client(clt_w, ResponseEncoder::COMMAND_NOT_IMPLEMENTED)
                            .await?;
                        continue;
                    }
                    self.send_burl_cmd(ups_w, clt_w, cmd_line, url).await?;
                    let _ = self
                        .recv_relay_rsp(self.config.data_termination_timeout, buf, ups_r, clt_w)
                        .await?;
                    return Ok(());
                }
                Command::NoOperation => {
                    self.send_cmd(ups_w, clt_w, cmd_line).await?;
                    let rsp = self
                        .recv_relay_rsp(self.config.response_wait_timeout, buf, ups_r, clt_w)
                        .await?;
                    if rsp != ReplyCode::OK {
                        return Err(ServerTaskError::UpstreamAppError(anyhow!(
                            "unexpected NOOP reply code {rsp}"
                        )));
                    }
                }
                Command::Reset => {
                    self.send_cmd(ups_w, clt_w, cmd_line).await?;
                    let rsp = self
                        .recv_relay_rsp(self.config.response_wait_timeout, buf, ups_r, clt_w)
                        .await?;
                    return if rsp != ReplyCode::OK {
                        Err(ServerTaskError::UpstreamAppError(anyhow!(
                            "unexpected RESET reply code {rsp}"
                        )))
                    } else {
                        Ok(())
                    };
                }
                Command::Quit => {
                    self.send_cmd(ups_w, clt_w, cmd_line).await?;
                    let _ = self
                        .recv_relay_rsp(self.config.response_wait_timeout, buf, ups_r, clt_w)
                        .await?;
                    self.quit = true;
                    return Ok(());
                }
                _ => {
                    self.send_error_to_client(clt_w, ResponseEncoder::BAD_SEQUENCE_OF_COMMANDS)
                        .await?;
                }
            }
        }
    }

    async fn recv_relay_rsp<CW, UR>(
        &mut self,
        recv_timeout: Duration,
        buf: &mut SmtpRelayBuf,
        ups_r: &mut UR,
        clt_w: &mut CW,
    ) -> ServerTaskResult<ReplyCode>
    where
        CW: AsyncWrite + Unpin,
        UR: AsyncRead + Unpin,
    {
        let mut rsp = ResponseParser::default();
        loop {
            buf.rsp_recv_buf.consume_line();
            let line = buf
                .rsp_recv_buf
                .read_rsp_line_with_feedback(recv_timeout, ups_r, clt_w, self.local_ip)
                .await?;
            let _msg = rsp
                .feed_line_with_feedback(line, clt_w, self.local_ip)
                .await?;

            clt_w
                .write_all_flush(line)
                .await
                .map_err(ServerTaskError::ClientTcpWriteFailed)?;

            if rsp.finished() {
                let code = rsp.code();
                return if code == ReplyCode::SERVICE_NOT_AVAILABLE {
                    Err(ServerTaskError::UpstreamAppUnavailable)
                } else {
                    Ok(code)
                };
            }
        }
    }

    async fn send_txt_data<CR, CW, UW>(
        &self,
        clt_r: &mut CR,
        clt_w: &mut CW,
        ups_w: &mut UW,
    ) -> ServerTaskResult<()>
    where
        CR: AsyncRead + Unpin,
        CW: AsyncWrite + Unpin,
        UW: AsyncWrite + Unpin,
    {
        if let Some(client) = self.ctx.audit_handle.icap_reqmod_client() {
            match client
                .smtp_message_adaptor(
                    self.ctx.server_config.limited_copy_config(),
                    self.ctx.idle_checker(),
                )
                .await
            {
                Ok(adapter) => {
                    return self
                        .send_txt_data_with_adaptation(clt_r, clt_w, ups_w, adapter)
                        .await;
                }
                Err(e) => {
                    if !client.bypass() {
                        return Err(ServerTaskError::InternalAdapterError(e));
                    }
                }
            }
        }

        let mut reader = TextDataReader::new(clt_r);
        self.transfer_data(&mut reader, ups_w).await
    }

    async fn send_txt_data_with_adaptation<CR, CW, UW>(
        &self,
        clt_r: &mut CR,
        clt_w: &mut CW,
        ups_w: &mut UW,
        mut adapter: SmtpMessageAdapter<ServerIdleChecker>,
    ) -> ServerTaskResult<()>
    where
        CR: AsyncRead + Unpin,
        CW: AsyncWrite + Unpin,
        UW: AsyncWrite + Unpin,
    {
        adapter.set_client_addr(self.ctx.task_notes.client_addr);
        if let Some(username) = self.ctx.raw_user_name() {
            adapter.set_client_username(username.clone());
        }
        if let Some(username) = self.ctx.tenant_user_name() {
            adapter.set_tenant_username(username.clone());
        }

        let mut adaptation_state = ReqmodAdaptationRunState::new(Instant::now());
        match adapter
            .xfer_data(
                &mut adaptation_state,
                clt_r,
                ups_w,
                &self.mail_from,
                &self.mail_to,
            )
            .await
        {
            Ok(ReqmodAdaptationEndState::OriginalTransferred) => Ok(()),
            Ok(ReqmodAdaptationEndState::AdaptedTransferred) => Ok(()),
            Ok(ReqmodAdaptationEndState::HttpErrResponse(rsp, body)) => {
                if let Some(mut body) = body {
                    let mut body_reader = body.body_reader();
                    let mut sinker = tokio::io::sink();
                    let _ = tokio::io::copy(&mut body_reader, &mut sinker).await;
                    if body_reader.trailer(128).await.is_ok() {
                        body.save_connection().await;
                    }
                }
                let client_rsp = ResponseEncoder::message_blocked(
                    self.local_ip,
                    format!("ICAP Response {} {}", rsp.status, rsp.reason),
                );
                let _ = client_rsp.write(clt_w).await;
                Err(ServerTaskError::InternalAdapterError(anyhow!(
                    "blocked by icap server: {} - {}",
                    rsp.status,
                    rsp.reason
                )))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn discard_bin_data<CR>(
        &self,
        buf: &mut SmtpRelayBuf,
        clt_r: &mut CR,
        size: usize,
    ) -> ServerTaskResult<()>
    where
        CR: AsyncRead + Unpin,
    {
        let cached = buf.cmd_recv_buf.consume_after_line(size).len();
        if cached >= size {
            return Ok(());
        }
        let mut reader = clt_r.take((size - cached) as u64);
        self.transfer_data(&mut reader, &mut tokio::io::sink())
            .await
    }

    async fn send_bin_data<CR, UW>(
        &self,
        buf: &mut SmtpRelayBuf,
        clt_r: &mut CR,
        ups_w: &mut UW,
        size: usize,
    ) -> ServerTaskResult<()>
    where
        CR: AsyncRead + Unpin,
        UW: AsyncWrite + Unpin,
    {
        let mut copy_size = size;
        let cache = buf.cmd_recv_buf.consume_after_line(size);
        if !cache.is_empty() {
            ups_w
                .write_all(cache)
                .await
                .map_err(ServerTaskError::UpstreamWriteFailed)?;
            copy_size -= cache.len();
            if copy_size == 0 {
                ups_w
                    .flush()
                    .await
                    .map_err(ServerTaskError::UpstreamWriteFailed)?;
                return Ok(());
            }
        }

        let mut reader = clt_r.take(copy_size as u64);

        self.transfer_data(&mut reader, ups_w).await
    }

    async fn transfer_data<CR, UW>(&self, clt_r: &mut CR, ups_w: &mut UW) -> ServerTaskResult<()>
    where
        CR: AsyncRead + Unpin,
        UW: AsyncWrite + Unpin,
    {
        let mut clt_to_ups =
            StreamCopy::new(clt_r, ups_w, &self.ctx.server_config.limited_copy_config());

        let mut idle_interval = self.ctx.idle_wheel.register();
        let mut idle_count = 0;

        loop {
            tokio::select! {
                biased;

                r = &mut clt_to_ups => {
                    return match r {
                        Ok(_) => {
                            // ups_w is already flushed
                            Ok(())
                        }
                        Err(StreamCopyError::ReadFailed(e)) => {
                            let _ = clt_to_ups.write_flush().await;
                            Err(ServerTaskError::ClientTcpReadFailed(e))
                        }
                        Err(StreamCopyError::WriteFailed(e)) => Err(ServerTaskError::UpstreamWriteFailed(e)),
                    };
                }
                n = idle_interval.tick() => {
                    if clt_to_ups.is_idle() {
                        idle_count += n;
                        if idle_count >= self.ctx.max_idle_count() {
                            return if clt_to_ups.no_cached_data() {
                                Err(ServerTaskError::ClientAppTimeout("idle while reading BDAT data"))
                            } else {
                                Err(ServerTaskError::UpstreamAppTimeout("idle while sending BDAT data"))
                            };
                        }
                    } else {
                        idle_count = 0;
                        clt_to_ups.reset_active();
                    }

                    if self.ctx.belongs_to_blocked_user() {
                        let _ = clt_to_ups.write_flush().await;
                        return Err(ServerTaskError::CanceledAsUserBlocked);
                    }

                    if self.ctx.server_force_quit() {
                        let _ = clt_to_ups.write_flush().await;
                        return Err(ServerTaskError::CanceledAsServerQuit)
                    }
                }
            }
        }
    }

    async fn send_data_cmd<UW, CW>(
        &self,
        ups_w: &mut UW,
        clt_w: &mut CW,
        cmd_line: &[u8],
    ) -> ServerTaskResult<()>
    where
        UW: AsyncWrite + Unpin,
        CW: AsyncWrite + Unpin,
    {
        self.send_cmd(ups_w, clt_w, cmd_line).await
    }

    async fn send_bdat_cmd<UW, CW>(
        &self,
        ups_w: &mut UW,
        clt_w: &mut CW,
        cmd_line: &[u8],
        _size: usize,
    ) -> ServerTaskResult<()>
    where
        UW: AsyncWrite + Unpin,
        CW: AsyncWrite + Unpin,
    {
        self.send_cmd(ups_w, clt_w, cmd_line).await
    }

    async fn send_burl_cmd<UW, CW>(
        &self,
        ups_w: &mut UW,
        clt_w: &mut CW,
        cmd_line: &[u8],
        _url: String,
    ) -> ServerTaskResult<()>
    where
        UW: AsyncWrite + Unpin,
        CW: AsyncWrite + Unpin,
    {
        self.send_cmd(ups_w, clt_w, cmd_line).await
    }

    async fn send_cmd<UW, CW>(
        &self,
        ups_w: &mut UW,
        clt_w: &mut CW,
        cmd_line: &[u8],
    ) -> ServerTaskResult<()>
    where
        UW: AsyncWrite + Unpin,
        CW: AsyncWrite + Unpin,
    {
        match ups_w.write_all_flush(cmd_line).await {
            Ok(_) => Ok(()),
            Err(e) => {
                let _ = ResponseEncoder::upstream_io_error(self.local_ip, &e)
                    .write(clt_w)
                    .await;
                Err(ServerTaskError::UpstreamWriteFailed(e))
            }
        }
    }

    async fn send_error_to_client<W>(
        &self,
        clt_w: &mut W,
        rsp_encoder: ResponseEncoder,
    ) -> ServerTaskResult<()>
    where
        W: AsyncWrite + Unpin,
    {
        rsp_encoder
            .write(clt_w)
            .await
            .map_err(ServerTaskError::ClientTcpWriteFailed)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use arc_swap::ArcSwapOption;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use vey_daemon::server::{ClientConnectionInfo, ServerQuitPolicy};
    use vey_io_ext::IdleWheel;
    use vey_smtp_proto::command::MailParam;
    use vey_types::metrics::{MetricTagMap, NodeName};
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

    fn test_context() -> StreamInspectContext<DummyCloseServerConfig> {
        let auditor = crate::audit::get_or_insert_default(&NodeName::default());
        let handle = auditor.build_handle().unwrap();
        let cc_info = ClientConnectionInfo::new(
            "127.0.0.1:12345".parse().unwrap(),
            "127.0.0.1:25".parse().unwrap(),
        );
        let task_notes = ServerTaskNotes::new(cc_info, None, Duration::ZERO);
        let mut egress_notes = EgressNotes {
            socket_type: Some(EgressSocketType::Direct),
            ..Default::default()
        };
        egress_notes.tcp.local = Some("127.0.0.1:12345".parse().unwrap());
        egress_notes.tcp.peer = Some("127.0.0.1:25".parse().unwrap());
        StreamInspectContext::new(
            handle,
            Arc::new(DummyCloseServerConfig::new(&NodeName::default(), None)),
            Arc::new(DummyServerStats::new()),
            Arc::new(ServerQuitPolicy::default()),
            IdleWheel::spawn(Duration::from_secs(60)),
            &task_notes,
            &egress_notes,
        )
    }

    #[tokio::test]
    async fn rejected_bdat_drops_a_buffered_chunk_and_forwards_the_next_command() {
        let ctx = test_context();
        let mut transaction = Transaction::new(
            &ctx,
            1,
            "127.0.0.1".parse().unwrap(),
            false,
            false,
            MailParam::default(),
        );
        let mut buf = SmtpRelayBuf::default();
        let (mut clt_in, mut clt_r) = tokio::io::duplex(4096);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(4096);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(4096);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(4096);

        // 4-byte chunk contains a newline, then QUIT is already buffered behind it.
        clt_in.write_all(b"BDAT 4\r\nAB\nCQUIT\r\n").await.unwrap();

        let intercept = transaction.relay(&mut buf, &mut clt_r, &mut clt_w, &mut ups_r, &mut ups_w);
        let drive = async {
            let mut read_buf = vec![0u8; 1024];
            let n = ups_out.read(&mut read_buf).await.unwrap();
            let forwarded = read_buf[..n].to_vec();
            ups_in.write_all(b"221 Bye\r\n").await.unwrap();
            forwarded
        };
        let (status, forwarded) = tokio::join!(intercept, drive);
        status.unwrap();
        assert!(transaction.quit());
        assert_eq!(forwarded, b"QUIT\r\n");

        drop(clt_w);
        let mut to_client = Vec::new();
        clt_out.read_to_end(&mut to_client).await.unwrap();
        assert_eq!(to_client, b"502 Command not implemented\r\n221 Bye\r\n");
    }

    async fn read_at_least(reader: &mut tokio::io::DuplexStream, n: usize) -> Vec<u8> {
        let mut got = Vec::new();
        let mut buf = [0u8; 256];
        while got.len() < n {
            let nr = reader.read(&mut buf).await.unwrap();
            assert!(nr > 0);
            got.extend_from_slice(&buf[..nr]);
        }
        got
    }

    #[tokio::test]
    async fn rcpt_and_data_are_forwarded_after_the_server_accepts_data() {
        let ctx = test_context();
        let mut transaction = Transaction::new(
            &ctx,
            1,
            "127.0.0.1".parse().unwrap(),
            false,
            false,
            MailParam::default(),
        );
        let mut relay_buf = SmtpRelayBuf::default();
        let (mut clt_in, mut clt_r) = tokio::io::duplex(4096);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(4096);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(4096);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(4096);

        clt_in.write_all(b"RCPT TO:<a@b>\r\n").await.unwrap();
        let intercept = transaction.relay(
            &mut relay_buf,
            &mut clt_r,
            &mut clt_w,
            &mut ups_r,
            &mut ups_w,
        );
        let drive = async {
            let got = read_at_least(&mut ups_out, b"RCPT TO:<a@b>\r\n".len()).await;
            assert_eq!(got, b"RCPT TO:<a@b>\r\n");
            ups_in.write_all(b"250 OK\r\n").await.unwrap();

            clt_in.write_all(b"DATA\r\n").await.unwrap();
            let got = read_at_least(&mut ups_out, b"DATA\r\n".len()).await;
            assert_eq!(got, b"DATA\r\n");
            ups_in.write_all(b"354 Start mail input\r\n").await.unwrap();

            clt_in.write_all(b"hello\r\n.\r\n").await.unwrap();
            let mut body = Vec::new();
            let mut buf = [0u8; 64];
            while !body.windows(5).any(|w| w == b"\r\n.\r\n") && !body.ends_with(b"\r\n.\r\n") {
                let n = ups_out.read(&mut buf).await.unwrap();
                body.extend_from_slice(&buf[..n]);
            }
            assert!(body.windows(7).any(|w| w == b"hello\r\n") || body.starts_with(b"hello\r\n"));
            ups_in.write_all(b"250 queued\r\n").await.unwrap();
        };
        let (status, _) = tokio::join!(intercept, drive);
        status.unwrap();
        assert!(!transaction.quit());
        drop(clt_w);
        let mut to_client = Vec::new();
        clt_out.read_to_end(&mut to_client).await.unwrap();
        assert_eq!(
            to_client,
            b"250 OK\r\n354 Start mail input\r\n250 queued\r\n"
        );
    }

    #[tokio::test]
    async fn data_without_start_mail_input_does_not_read_a_body() {
        let ctx = test_context();
        let mut transaction = Transaction::new(
            &ctx,
            1,
            "127.0.0.1".parse().unwrap(),
            false,
            false,
            MailParam::default(),
        );
        let mut relay_buf = SmtpRelayBuf::default();
        let (mut clt_in, mut clt_r) = tokio::io::duplex(4096);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(4096);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(4096);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(4096);

        clt_in.write_all(b"DATA\r\nQUIT\r\n").await.unwrap();
        let intercept = transaction.relay(
            &mut relay_buf,
            &mut clt_r,
            &mut clt_w,
            &mut ups_r,
            &mut ups_w,
        );
        let drive = async {
            let data = read_at_least(&mut ups_out, b"DATA\r\n".len()).await;
            assert_eq!(data, b"DATA\r\n");
            ups_in.write_all(b"554 no\r\n").await.unwrap();
            let quit = read_at_least(&mut ups_out, b"QUIT\r\n".len()).await;
            assert_eq!(quit, b"QUIT\r\n");
            ups_in.write_all(b"221 Bye\r\n").await.unwrap();
        };
        let (status, _) = tokio::join!(intercept, drive);
        status.unwrap();
        drop(clt_w);
        let mut to_client = Vec::new();
        clt_out.read_to_end(&mut to_client).await.unwrap();
        assert_eq!(to_client, b"554 no\r\n221 Bye\r\n");
    }

    #[tokio::test]
    async fn reset_ends_the_transaction_when_upstream_accepts() {
        let ctx = test_context();
        let mut transaction = Transaction::new(
            &ctx,
            1,
            "127.0.0.1".parse().unwrap(),
            false,
            false,
            MailParam::default(),
        );
        let mut relay_buf = SmtpRelayBuf::default();
        let (mut clt_in, mut clt_r) = tokio::io::duplex(1024);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(1024);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(1024);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(1024);
        clt_in.write_all(b"RSET\r\n").await.unwrap();
        let intercept = transaction.relay(
            &mut relay_buf,
            &mut clt_r,
            &mut clt_w,
            &mut ups_r,
            &mut ups_w,
        );
        let drive = async {
            let got = read_at_least(&mut ups_out, b"RSET\r\n".len()).await;
            assert_eq!(got, b"RSET\r\n");
            ups_in.write_all(b"250 OK\r\n").await.unwrap();
        };
        let (status, _) = tokio::join!(intercept, drive);
        status.unwrap();
        assert!(!transaction.quit());
        drop(clt_w);
        let mut to_client = Vec::new();
        clt_out.read_to_end(&mut to_client).await.unwrap();
        assert_eq!(to_client, b"250 OK\r\n");
    }

    #[tokio::test]
    async fn an_unexpected_noop_reply_fails_the_transaction() {
        let ctx = test_context();
        let mut transaction = Transaction::new(
            &ctx,
            1,
            "127.0.0.1".parse().unwrap(),
            false,
            false,
            MailParam::default(),
        );
        let mut relay_buf = SmtpRelayBuf::default();
        let (mut clt_in, mut clt_r) = tokio::io::duplex(1024);
        let (mut clt_w, _clt_out) = tokio::io::duplex(1024);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(1024);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(1024);
        clt_in.write_all(b"NOOP\r\n").await.unwrap();
        let intercept = transaction.relay(
            &mut relay_buf,
            &mut clt_r,
            &mut clt_w,
            &mut ups_r,
            &mut ups_w,
        );
        let drive = async {
            let got = read_at_least(&mut ups_out, b"NOOP\r\n".len()).await;
            assert_eq!(got, b"NOOP\r\n");
            ups_in.write_all(b"550 no\r\n").await.unwrap();
        };
        let (status, _) = tokio::join!(intercept, drive);
        assert!(matches!(status, Err(ServerTaskError::UpstreamAppError(_))));
    }

    #[tokio::test]
    async fn mail_during_a_transaction_is_rejected_locally() {
        let ctx = test_context();
        let mut transaction = Transaction::new(
            &ctx,
            1,
            "127.0.0.1".parse().unwrap(),
            false,
            false,
            MailParam::default(),
        );
        let mut relay_buf = SmtpRelayBuf::default();
        let (mut clt_in, mut clt_r) = tokio::io::duplex(1024);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(1024);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(1024);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(1024);
        clt_in.write_all(b"MAIL FROM:<>\r\nQUIT\r\n").await.unwrap();
        let intercept = transaction.relay(
            &mut relay_buf,
            &mut clt_r,
            &mut clt_w,
            &mut ups_r,
            &mut ups_w,
        );
        let drive = async {
            let got = read_at_least(&mut ups_out, b"QUIT\r\n".len()).await;
            assert_eq!(got, b"QUIT\r\n");
            ups_in.write_all(b"221 Bye\r\n").await.unwrap();
        };
        let (status, _) = tokio::join!(intercept, drive);
        status.unwrap();
        drop(clt_w);
        let mut to_client = Vec::new();
        clt_out.read_to_end(&mut to_client).await.unwrap();
        assert_eq!(to_client, b"503 Bad sequence of commands\r\n221 Bye\r\n");
    }

    #[tokio::test]
    async fn an_accepted_last_bdat_forwards_the_buffered_chunk() {
        let ctx = test_context();
        let mut transaction = Transaction::new(
            &ctx,
            1,
            "127.0.0.1".parse().unwrap(),
            true,
            false,
            MailParam::default(),
        );
        let mut relay_buf = SmtpRelayBuf::default();
        let (mut clt_in, mut clt_r) = tokio::io::duplex(1024);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(1024);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(1024);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(1024);
        let sent = b"BDAT 4 LAST\r\nABCD";
        clt_in.write_all(sent).await.unwrap();
        let intercept = transaction.relay(
            &mut relay_buf,
            &mut clt_r,
            &mut clt_w,
            &mut ups_r,
            &mut ups_w,
        );
        let drive = async {
            let got = read_at_least(&mut ups_out, sent.len()).await;
            assert_eq!(got, sent);
            ups_in.write_all(b"250 queued\r\n").await.unwrap();
        };
        let (status, _) = tokio::join!(intercept, drive);
        status.unwrap();
        assert!(!transaction.quit());
        drop(clt_w);
        let mut to_client = Vec::new();
        clt_out.read_to_end(&mut to_client).await.unwrap();
        assert_eq!(to_client, b"250 queued\r\n");
    }

    #[tokio::test]
    async fn burl_is_rejected_when_it_is_not_allowed() {
        let ctx = test_context();
        let mut transaction = Transaction::new(
            &ctx,
            1,
            "127.0.0.1".parse().unwrap(),
            false,
            false,
            MailParam::default(),
        );
        let mut relay_buf = SmtpRelayBuf::default();
        let (mut clt_in, mut clt_r) = tokio::io::duplex(1024);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(1024);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(1024);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(1024);
        clt_in
            .write_all(b"BURL imap://user@imap.example.com/INBOX;UID=1\r\nQUIT\r\n")
            .await
            .unwrap();
        let intercept = transaction.relay(
            &mut relay_buf,
            &mut clt_r,
            &mut clt_w,
            &mut ups_r,
            &mut ups_w,
        );
        let drive = async {
            let got = read_at_least(&mut ups_out, b"QUIT\r\n".len()).await;
            assert_eq!(got, b"QUIT\r\n");
            ups_in.write_all(b"221 Bye\r\n").await.unwrap();
        };
        let (status, _) = tokio::join!(intercept, drive);
        status.unwrap();
        drop(clt_w);
        let mut to_client = Vec::new();
        clt_out.read_to_end(&mut to_client).await.unwrap();
        assert_eq!(to_client, b"502 Command not implemented\r\n221 Bye\r\n");
    }
}
