/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use std::net::IpAddr;

use tokio::io::{AsyncRead, AsyncWrite};

use vey_dpi::SmtpInterceptionConfig;
use vey_io_ext::{LimitedWriteExt, LineRecvBuf};
use vey_smtp_proto::command::{Command, MailParam};
use vey_smtp_proto::response::{ReplyCode, ResponseEncoder, ResponseParser};

use super::{
    CommandLineRecvExt, InitializedExtensions, Initiation, ResponseLineRecvExt, ResponseParseExt,
    SmtpRelayBuf,
};
use crate::serve::{ServerTaskError, ServerTaskResult};

#[derive(Debug)]
pub(super) enum ForwardNextAction {
    Quit,
    StartTls,
    ReverseConnection,
    SetExtensions(InitializedExtensions),
    MailTransport(MailParam),
}

pub(super) struct Forward<'a> {
    config: &'a SmtpInterceptionConfig,
    local_ip: IpAddr,
    allow_odmr: bool,
    allow_starttls: bool,
    auth_end: bool,
}

impl<'a> Forward<'a> {
    pub(super) fn new(
        config: &'a SmtpInterceptionConfig,
        local_ip: IpAddr,
        allow_odmr: bool,
        allow_starttls: bool,
    ) -> Self {
        Forward {
            config,
            local_ip,
            allow_odmr,
            allow_starttls,
            auth_end: false,
        }
    }

    pub(super) async fn relay<CR, CW, UR, UW>(
        &mut self,
        buf: &mut SmtpRelayBuf,
        clt_r: &mut CR,
        clt_w: &mut CW,
        ups_r: &mut UR,
        ups_w: &mut UW,
    ) -> ServerTaskResult<ForwardNextAction>
    where
        CR: AsyncRead + Unpin,
        CW: AsyncWrite + Unpin,
        UR: AsyncRead + Unpin,
        UW: AsyncWrite + Unpin,
    {
        loop {
            buf.cmd_recv_buf.consume_line();
            let (cmd, cmd_line) = buf
                .cmd_recv_buf
                .recv_cmd(self.config.command_wait_timeout, clt_r, clt_w)
                .await?;

            match cmd {
                Command::Hello(_)
                | Command::Recipient(_)
                | Command::Data
                | Command::BinaryData(_)
                | Command::LastBinaryData(_)
                | Command::DataByUrl(_)
                | Command::LastDataByUrl(_) => {
                    self.send_error_to_client(clt_w, ResponseEncoder::BAD_SEQUENCE_OF_COMMANDS)
                        .await?;
                }
                Command::Quit => {
                    self.send_cmd(ups_w, clt_w, cmd_line).await?;
                    let _ = self.recv_relay_rsp(buf, ups_r, clt_w).await?;
                    return Ok(ForwardNextAction::Quit);
                }
                Command::StartTls => {
                    if !self.allow_starttls {
                        self.send_error_to_client(clt_w, ResponseEncoder::COMMAND_NOT_IMPLEMENTED)
                            .await?;
                        continue;
                    }
                    self.send_cmd(ups_w, clt_w, cmd_line).await?;
                    let rsp = self.recv_relay_rsp(buf, ups_r, clt_w).await?;
                    if rsp == ReplyCode::SERVICE_READY {
                        return Ok(ForwardNextAction::StartTls);
                    }
                }
                Command::Auth => {
                    if self.auth_end {
                        self.send_error_to_client(clt_w, ResponseEncoder::BAD_SEQUENCE_OF_COMMANDS)
                            .await?;
                        continue;
                    }
                    self.send_cmd(ups_w, clt_w, cmd_line).await?;
                    self.recv_relay_auth(buf, clt_r, clt_w, ups_r, ups_w)
                        .await?;
                }
                Command::AuthenticatedTurn => {
                    if !self.allow_odmr {
                        self.send_error_to_client(clt_w, ResponseEncoder::COMMAND_NOT_IMPLEMENTED)
                            .await?;
                        continue;
                    }
                    if !self.auth_end {
                        self.send_error_to_client(clt_w, ResponseEncoder::AUTHENTICATION_REQUIRED)
                            .await?;
                        continue;
                    }
                    self.send_cmd(ups_w, clt_w, cmd_line).await?;
                    // a max 10min timeout according to RFC2645
                    let rsp = self.recv_relay_rsp(buf, ups_r, clt_w).await?;
                    if rsp == ReplyCode::OK {
                        return Ok(ForwardNextAction::ReverseConnection);
                    }
                }
                Command::ExtendHello(_host) => {
                    self.send_cmd(ups_w, clt_w, cmd_line).await?;
                    let mut initialization = Initiation::new(self.config, self.local_ip, true);
                    if initialization
                        .recv_relay_check_rsp(&mut buf.rsp_recv_buf, ups_r, clt_w)
                        .await?
                        .is_some()
                    {
                        let (_, extensions) = initialization.into_parts();
                        return Ok(ForwardNextAction::SetExtensions(extensions));
                    }
                }
                Command::Mail(param) => {
                    self.send_cmd(ups_w, clt_w, cmd_line).await?;
                    let rsp = self.recv_relay_rsp(buf, ups_r, clt_w).await?;
                    if rsp == ReplyCode::OK {
                        return Ok(ForwardNextAction::MailTransport(param));
                    }
                }
                _ => {
                    self.send_cmd(ups_w, clt_w, cmd_line).await?;
                    self.recv_relay_rsp(buf, ups_r, clt_w).await?;
                }
            }
        }
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

    async fn recv_relay_rsp<CW, UR>(
        &mut self,
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
                .read_rsp_line_with_feedback(
                    self.config.response_wait_timeout,
                    ups_r,
                    clt_w,
                    self.local_ip,
                )
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

    async fn recv_relay_auth<CR, CW, UR, UW>(
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
        loop {
            let rsp = self.recv_relay_rsp(buf, ups_r, clt_w).await?;
            match rsp {
                ReplyCode::AUTHENTICATION_SUCCESSFUL => {
                    self.auth_end = true;
                    return Ok(());
                }
                ReplyCode::AUTH_CONTINUE => {}
                _ => return Ok(()),
            }

            let mut recv_buf = LineRecvBuf::<{ Command::MAX_CONTINUE_LINE_SIZE }>::default();
            match recv_buf
                .read_line_with_timeout(clt_r, self.config.command_wait_timeout)
                .await
            {
                Ok(line) => {
                    ups_w
                        .write_all_flush(line)
                        .await
                        .map_err(ServerTaskError::UpstreamWriteFailed)?;
                    recv_buf.consume_line();
                }
                Err(e) => {
                    let e = LineRecvBuf::<{ Command::MAX_CONTINUE_LINE_SIZE }>::handle_line_error(
                        e, clt_w,
                    )
                    .await;
                    return Err(e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use vey_dpi::SmtpInterceptionConfig;

    use super::*;
    use crate::serve::ServerTaskError;

    fn local_ip() -> std::net::IpAddr {
        "127.0.0.1".parse().unwrap()
    }

    fn new_forward<'a>(
        config: &'a SmtpInterceptionConfig,
        allow_odmr: bool,
        allow_starttls: bool,
    ) -> Forward<'a> {
        Forward::new(config, local_ip(), allow_odmr, allow_starttls)
    }

    async fn relay_until_next(
        forward: &mut Forward<'_>,
        cmd: &[u8],
        rsp: &[u8],
    ) -> (ForwardNextAction, Vec<u8>, Vec<u8>) {
        let mut buf = SmtpRelayBuf::default();
        let (mut clt_in, mut clt_r) = tokio::io::duplex(4096);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(4096);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(4096);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(4096);

        clt_in.write_all(cmd).await.unwrap();

        let intercept = forward.relay(&mut buf, &mut clt_r, &mut clt_w, &mut ups_r, &mut ups_w);
        let drive = async {
            let mut read_buf = vec![0u8; 1024];
            let n = ups_out.read(&mut read_buf).await.unwrap();
            let forwarded = read_buf[..n].to_vec();
            ups_in.write_all(rsp).await.unwrap();
            forwarded
        };
        let (status, forwarded) = tokio::join!(intercept, drive);
        let action = status.unwrap();
        drop(clt_w);
        let mut to_client = Vec::new();
        clt_out.read_to_end(&mut to_client).await.unwrap();
        (action, to_client, forwarded)
    }

    #[tokio::test]
    async fn quit_returns_quit_action() {
        let config = SmtpInterceptionConfig::default();
        let mut forward = new_forward(&config, false, false);
        let cmd = b"QUIT\r\n";
        let (action, to_client, forwarded) =
            relay_until_next(&mut forward, cmd, b"221 Bye\r\n").await;
        assert!(matches!(action, ForwardNextAction::Quit));
        assert_eq!(forwarded, cmd);
        assert_eq!(to_client, b"221 Bye\r\n");
    }

    #[tokio::test]
    async fn starttls_ready_returns_starttls_action() {
        let config = SmtpInterceptionConfig::default();
        let mut forward = new_forward(&config, false, true);
        let cmd = b"STARTTLS\r\n";
        let (action, to_client, forwarded) =
            relay_until_next(&mut forward, cmd, b"220 Ready to start TLS\r\n").await;
        assert!(matches!(action, ForwardNextAction::StartTls));
        assert_eq!(forwarded, cmd);
        assert_eq!(to_client, b"220 Ready to start TLS\r\n");
    }

    #[tokio::test]
    async fn starttls_rejected_when_not_advertised() {
        let config = SmtpInterceptionConfig::default();
        let mut forward = new_forward(&config, false, false);
        let mut buf = SmtpRelayBuf::default();
        let (mut clt_in, mut clt_r) = tokio::io::duplex(4096);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(4096);
        let (_ups_in, mut ups_r) = tokio::io::duplex(4096);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(4096);

        clt_in.write_all(b"STARTTLS\r\n").await.unwrap();
        drop(clt_in);

        let intercept = forward.relay(&mut buf, &mut clt_r, &mut clt_w, &mut ups_r, &mut ups_w);
        let drive = async {
            let mut read_buf = [0u8; 128];
            let n = clt_out.read(&mut read_buf).await.unwrap();
            read_buf[..n].to_vec()
        };
        let (status, to_client) = tokio::join!(intercept, drive);
        assert!(matches!(status, Err(ServerTaskError::ClosedByClient)));
        assert_eq!(to_client, b"502 Command not implemented\r\n");
        drop(ups_w);
        let mut forwarded = Vec::new();
        ups_out.read_to_end(&mut forwarded).await.unwrap();
        assert!(
            forwarded.is_empty(),
            "upstream {}",
            String::from_utf8_lossy(&forwarded)
        );
    }

    #[tokio::test]
    async fn mail_ok_returns_mail_transport() {
        let config = SmtpInterceptionConfig::default();
        let mut forward = new_forward(&config, false, false);
        let cmd = b"MAIL FROM:<user@example.com>\r\n";
        let (action, to_client, forwarded) =
            relay_until_next(&mut forward, cmd, b"250 2.1.0 OK\r\n").await;
        match action {
            ForwardNextAction::MailTransport(param) => {
                assert_eq!(param.reverse_path(), "<user@example.com>");
            }
            other => panic!("unexpected action {other:?}"),
        }
        assert_eq!(forwarded, cmd);
        assert_eq!(to_client, b"250 2.1.0 OK\r\n");
    }

    #[tokio::test]
    async fn data_and_rcpt_are_rejected_before_mail() {
        let config = SmtpInterceptionConfig::default();
        let mut forward = new_forward(&config, false, false);
        let mut buf = SmtpRelayBuf::default();
        let (mut clt_in, mut clt_r) = tokio::io::duplex(4096);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(4096);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(4096);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(4096);

        clt_in.write_all(b"DATA\r\n").await.unwrap();
        clt_in
            .write_all(b"RCPT TO:<user@example.com>\r\n")
            .await
            .unwrap();
        clt_in.write_all(b"QUIT\r\n").await.unwrap();

        let intercept = forward.relay(&mut buf, &mut clt_r, &mut clt_w, &mut ups_r, &mut ups_w);
        let drive = async {
            let mut read_buf = [0u8; 128];
            let n = ups_out.read(&mut read_buf).await.unwrap();
            assert_eq!(&read_buf[..n], b"QUIT\r\n");
            ups_in.write_all(b"221 Bye\r\n").await.unwrap();
        };
        let (status, _) = tokio::join!(intercept, drive);
        assert!(matches!(status.unwrap(), ForwardNextAction::Quit));
        drop(clt_w);
        let mut to_client = Vec::new();
        clt_out.read_to_end(&mut to_client).await.unwrap();
        assert_eq!(
            to_client,
            concat!(
                "503 Bad sequence of commands\r\n",
                "503 Bad sequence of commands\r\n",
                "221 Bye\r\n",
            )
            .as_bytes()
        );
    }

    #[tokio::test]
    async fn auth_success_then_second_auth_is_rejected() {
        let config = SmtpInterceptionConfig::default();
        let mut forward = new_forward(&config, false, false);
        let mut buf = SmtpRelayBuf::default();
        let (mut clt_in, mut clt_r) = tokio::io::duplex(4096);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(4096);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(4096);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(4096);

        let auth = b"AUTH PLAIN AHVzZXIAcGFzcw==\r\n";
        clt_in.write_all(auth).await.unwrap();
        clt_in
            .write_all(b"AUTH PLAIN AHVzZXIAcGFzcw==\r\n")
            .await
            .unwrap();
        clt_in.write_all(b"QUIT\r\n").await.unwrap();

        let intercept = forward.relay(&mut buf, &mut clt_r, &mut clt_w, &mut ups_r, &mut ups_w);
        let drive = async {
            let mut read_buf = [0u8; 128];
            let n = ups_out.read(&mut read_buf).await.unwrap();
            assert_eq!(&read_buf[..n], auth);
            ups_in
                .write_all(b"235 2.7.0 Authentication successful\r\n")
                .await
                .unwrap();
            let n = ups_out.read(&mut read_buf).await.unwrap();
            assert_eq!(&read_buf[..n], b"QUIT\r\n");
            ups_in.write_all(b"221 Bye\r\n").await.unwrap();
        };
        let (status, _) = tokio::join!(intercept, drive);
        assert!(matches!(status.unwrap(), ForwardNextAction::Quit));
        drop(clt_w);
        let mut to_client = Vec::new();
        clt_out.read_to_end(&mut to_client).await.unwrap();
        assert_eq!(
            to_client,
            concat!(
                "235 2.7.0 Authentication successful\r\n",
                "503 Bad sequence of commands\r\n",
                "221 Bye\r\n",
            )
            .as_bytes()
        );
    }

    #[tokio::test]
    async fn atrn_without_auth_is_rejected_locally() {
        let config = SmtpInterceptionConfig::default();
        let mut forward = new_forward(&config, true, false);
        let mut buf = SmtpRelayBuf::default();
        let (mut clt_in, mut clt_r) = tokio::io::duplex(4096);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(4096);
        let (_ups_in, mut ups_r) = tokio::io::duplex(4096);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(4096);

        clt_in.write_all(b"ATRN\r\n").await.unwrap();
        drop(clt_in);

        let intercept = forward.relay(&mut buf, &mut clt_r, &mut clt_w, &mut ups_r, &mut ups_w);
        let drive = async {
            let mut read_buf = [0u8; 128];
            let n = clt_out.read(&mut read_buf).await.unwrap();
            read_buf[..n].to_vec()
        };
        let (status, to_client) = tokio::join!(intercept, drive);
        assert!(matches!(status, Err(ServerTaskError::ClosedByClient)));
        assert_eq!(to_client, b"530 Authentication required\r\n");
        drop(ups_w);
        let mut forwarded = Vec::new();
        ups_out.read_to_end(&mut forwarded).await.unwrap();
        assert!(
            forwarded.is_empty(),
            "upstream {}",
            String::from_utf8_lossy(&forwarded)
        );
    }

    #[tokio::test]
    async fn ehlo_sets_extensions() {
        let config = SmtpInterceptionConfig::default();
        let mut forward = new_forward(&config, false, true);
        let cmd = b"EHLO client.example\r\n";
        let rsp = concat!(
            "250-mail.example.com Hello\r\n",
            "250-STARTTLS\r\n",
            "250 PIPELINING\r\n",
        );
        let (action, to_client, forwarded) =
            relay_until_next(&mut forward, cmd, rsp.as_bytes()).await;
        match action {
            ForwardNextAction::SetExtensions(ext) => {
                assert!(!ext.allow_starttls(true));
            }
            other => panic!("unexpected action {other:?}"),
        }
        assert_eq!(forwarded, cmd);
        assert_eq!(
            to_client,
            concat!("250-mail.example.com Hello\r\n", "250 PIPELINING\r\n",).as_bytes()
        );
    }
}
