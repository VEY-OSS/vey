/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use std::net::IpAddr;
use std::str;

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use vey_dpi::SmtpInterceptionConfig;
use vey_io_ext::{LimitedWriteExt, LineRecvBuf};
use vey_smtp_proto::command::Command;
use vey_smtp_proto::response::{ReplyCode, ResponseEncoder, ResponseParser};
use vey_types::net::Host;

use super::{CommandLineRecvExt, ResponseLineRecvExt, ResponseParseExt};
use crate::serve::{ServerTaskError, ServerTaskResult};

#[derive(Default, Debug)]
pub(super) struct InitializedExtensions {
    odmr: bool,
    starttls: bool,
    chunking: bool,
    burl: bool,
}

impl InitializedExtensions {
    pub(super) fn allow_odmr(&self, config: &SmtpInterceptionConfig) -> bool {
        self.odmr && config.allow_on_demand_mail_relay
    }

    pub(super) fn allow_starttls(&self, from_starttls: bool) -> bool {
        self.starttls && !from_starttls
    }

    pub(super) fn allow_chunking(&self, config: &SmtpInterceptionConfig) -> bool {
        self.chunking && config.allow_data_chunking
    }

    pub(super) fn allow_burl(&self, config: &SmtpInterceptionConfig) -> bool {
        self.burl && config.allow_burl_data
    }
}

pub(super) struct Initiation<'a> {
    config: &'a SmtpInterceptionConfig,
    local_ip: IpAddr,
    from_starttls: bool,
    client_host: Host,
    server_ext: InitializedExtensions,
}

impl<'a> Initiation<'a> {
    pub(super) fn new(
        config: &'a SmtpInterceptionConfig,
        local_ip: IpAddr,
        from_starttls: bool,
    ) -> Self {
        Initiation {
            config,
            local_ip,
            from_starttls,
            client_host: Host::empty(),
            server_ext: InitializedExtensions::default(),
        }
    }

    pub(super) fn into_parts(self) -> (Host, InitializedExtensions) {
        (self.client_host, self.server_ext)
    }

    pub(super) async fn relay<CR, CW, UR, UW>(
        &mut self,
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
        let mut cmd_recv_buf = LineRecvBuf::<{ Command::MAX_LINE_SIZE }>::default();
        let mut rsp_recv_buf = LineRecvBuf::<{ ResponseParser::MAX_LINE_SIZE }>::default();

        loop {
            cmd_recv_buf.consume_line();
            let (cmd, cmd_line) = cmd_recv_buf
                .recv_cmd(self.config.command_wait_timeout, clt_r, clt_w)
                .await?;

            match cmd {
                Command::ExtendHello(host) => {
                    self.client_host = host;
                    self.send_cmd(ups_w, clt_w, cmd_line).await?;
                }
                Command::Hello(host) => {
                    self.client_host = host;
                    self.send_cmd(ups_w, clt_w, cmd_line).await?;
                }
                _ => {
                    self.send_error_to_client(clt_w, ResponseEncoder::BAD_SEQUENCE_OF_COMMANDS)
                        .await?;
                    continue;
                }
            }

            if self
                .recv_relay_check_rsp(&mut rsp_recv_buf, ups_r, clt_w)
                .await?
                .is_some()
            {
                return Ok(());
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

    pub(super) async fn recv_relay_check_rsp<CW, UR>(
        &mut self,
        rsp_recv_buf: &mut LineRecvBuf<{ ResponseParser::MAX_LINE_SIZE }>,
        ups_r: &mut UR,
        clt_w: &mut CW,
    ) -> ServerTaskResult<Option<()>>
    where
        CW: AsyncWrite + Unpin,
        UR: AsyncRead + Unpin,
    {
        let mut rsp = ResponseParser::default();
        loop {
            rsp_recv_buf.consume_line();
            let line = rsp_recv_buf
                .read_rsp_line_with_feedback(
                    self.config.response_wait_timeout,
                    ups_r,
                    clt_w,
                    self.local_ip,
                )
                .await?;
            let msg = rsp
                .feed_line_with_feedback(line, clt_w, self.local_ip)
                .await?;

            match rsp.code() {
                ReplyCode::OK => {
                    if rsp.is_first_line() || self.allow_extension(msg) {
                        clt_w
                            .write_all(line)
                            .await
                            .map_err(ServerTaskError::ClientTcpWriteFailed)?;
                    }

                    if rsp.finished() {
                        clt_w
                            .flush()
                            .await
                            .map_err(ServerTaskError::ClientTcpWriteFailed)?;
                        return Ok(Some(()));
                    }
                }
                ReplyCode::SERVICE_NOT_AVAILABLE => {
                    clt_w
                        .write_all(line)
                        .await
                        .map_err(ServerTaskError::ClientTcpWriteFailed)?;

                    if rsp.finished() {
                        let _ = clt_w.flush().await;
                        return Err(ServerTaskError::UpstreamAppUnavailable);
                    }
                }
                _ => {
                    clt_w
                        .write_all(line)
                        .await
                        .map_err(ServerTaskError::ClientTcpWriteFailed)?;

                    if rsp.finished() {
                        clt_w
                            .flush()
                            .await
                            .map_err(ServerTaskError::ClientTcpWriteFailed)?;
                        return Ok(None);
                    }
                }
            }
        }
    }

    fn allow_extension(&mut self, msg: &[u8]) -> bool {
        if let Some(p) = memchr::memchr(b' ', msg) {
            let Ok(keyword) = str::from_utf8(&msg[..p]) else {
                return false;
            };

            match keyword.to_uppercase().as_str() {
                // Message Size Declaration, RFC1870, TODO use this max message limit ?
                "SIZE" => true,
                // Deliver By, RFC2852, add a MAIL BY param key
                "DELIVERBY" => true,
                // No Soliciting, RFC3865, add a MAIL param key
                "NO-SOLICITING" => true,
                // Authentication, RFC4954, add AUTH command
                "AUTH" => true,
                // BURL, RFC4468, add BURL command
                "BURL" => {
                    self.server_ext.burl = true;
                    self.config.allow_burl_data
                }
                // Future Message Release, RFC4865, add MAIL param keys
                "FUTURERELEASE" => true,
                // Priority Message Handling, RFC6710, add a MAIL param key
                "MT-PRIORITY" => true,
                // LIMITS, RFC9422
                "LIMITS" => true,
                _ => false,
            }
        } else {
            let Ok(keyword) = str::from_utf8(msg) else {
                return false;
            };

            match keyword.to_uppercase().as_str() {
                // Expand the mailing list, RFC5321, add EXPN command
                "EXPN" => true,
                // Supply helpful information, RFC5321, add HELP command
                "HELP" => true,
                // 8bit-MIMEtransport, RFC6152, add a MAIL BODY param value
                "8BITMIME" => true,
                // Message Size Declaration, RFC1870
                "SIZE" => true,
                // Verbose
                "VERB" => true,
                // One message transaction only
                "ONEX" => true,
                // CHUNKING, RFC3030, add BDAT command
                "CHUNKING" => {
                    self.server_ext.chunking = true;
                    self.config.allow_data_chunking
                }
                // BINARYMIME, RFC3030, add a MAIL BODY param value, require CHUNKING
                "BINARYMIME" => self.config.allow_data_chunking,
                // Deliver By, RFC2852, add a MAIL BY param key
                "DELIVERBY" => true,
                // Pipelining, RFC2920
                "PIPELINING" => true,
                // Delivery Status Notification, RFC3461, add param keys to RCPT and MAIL
                "DSN" => true,
                // Remote Queue Processing Declaration, RFC1985, add ETRN command
                "ETRN" => true,
                // Enhanced-Status-Codes, RFC2034, add status code preface to response
                "ENHANCEDSTATUSCODES" => false,
                // STARTTLS, RFC3207, add STARTTLS command
                "STARTTLS" => {
                    self.server_ext.starttls = true;
                    !self.from_starttls
                }
                // No Soliciting, RFC3865, add a MAIL param key
                "NO-SOLICITING" => true,
                // Message Tracking, RFC3885, add a MAIL MTRK param key
                "MTRK" => true,
                // BURL, RFC4468, add BURL command, no param means AUTH is required
                "BURL" => {
                    self.server_ext.burl = true;
                    self.config.allow_burl_data
                }
                // Content-Conversion-Permission, RFC4141, add a MAIL param key
                "CONPERM" => true,
                // Content-Negotiation, RFC4141, add a RCPT param key
                "CONNEG" => true,
                // Internationalized Email, RFC6531, add MAIL/VRFY/EXPN param key
                "SMTPUTF8" => true,
                // Priority Message Handling, RFC6710, add a MAIL param key
                "MT-PRIORITY" => true,
                // Require Recipient Valid Since, RFC7293, add a RCPT param key
                "RRVS" => true,
                // Require TLS, RFC8689, add a MAIL param key
                "REQUIRETLS" => true,
                // LIMITS, RFC9422
                "LIMITS" => true,
                // On-Demand Mail Relay, RFC2645, change the protocol
                "ATRN" => {
                    self.server_ext.odmr = true;
                    self.config.allow_on_demand_mail_relay
                }
                _ => false,
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

    fn local_ip() -> IpAddr {
        "127.0.0.1".parse().unwrap()
    }

    async fn ehlo_relay(
        config: &SmtpInterceptionConfig,
        from_starttls: bool,
        cmd: &[u8],
        rsp: &[u8],
    ) -> (Host, InitializedExtensions, Vec<u8>, Vec<u8>) {
        let mut initiation = Initiation::new(config, local_ip(), from_starttls);
        let (mut clt_in, mut clt_r) = tokio::io::duplex(4096);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(4096);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(4096);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(4096);

        clt_in.write_all(cmd).await.unwrap();

        let intercept = initiation.relay(&mut clt_r, &mut clt_w, &mut ups_r, &mut ups_w);
        let drive = async {
            let mut buf = vec![0u8; 1024];
            let n = ups_out.read(&mut buf).await.unwrap();
            let forwarded = buf[..n].to_vec();
            ups_in.write_all(rsp).await.unwrap();
            forwarded
        };
        let (status, forwarded) = tokio::join!(intercept, drive);
        status.unwrap();
        drop(clt_w);
        let mut to_client = Vec::new();
        clt_out.read_to_end(&mut to_client).await.unwrap();
        let (host, ext) = initiation.into_parts();
        (host, ext, to_client, forwarded)
    }

    #[tokio::test]
    async fn ehlo_ok_records_client_host_and_forwards() {
        let config = SmtpInterceptionConfig::default();
        let cmd = b"EHLO client.example\r\n";
        let (host, ext, to_client, forwarded) =
            ehlo_relay(&config, false, cmd, b"250 mail.example.com Hello\r\n").await;
        assert_eq!(host.to_string(), "client.example");
        assert!(!ext.allow_starttls(false));
        assert_eq!(forwarded, cmd);
        assert_eq!(to_client, b"250 mail.example.com Hello\r\n");
    }

    #[tokio::test]
    async fn helo_ok_is_forwarded() {
        let config = SmtpInterceptionConfig::default();
        let cmd = b"HELO client.example\r\n";
        let (host, _, to_client, forwarded) =
            ehlo_relay(&config, false, cmd, b"250 mail.example.com Hello\r\n").await;
        assert_eq!(host.to_string(), "client.example");
        assert_eq!(forwarded, cmd);
        assert_eq!(to_client, b"250 mail.example.com Hello\r\n");
    }

    #[tokio::test]
    async fn ehlo_filters_unsupported_extensions() {
        let config = SmtpInterceptionConfig::default();
        let cmd = b"EHLO client.example\r\n";
        let rsp = concat!(
            "250-mail.example.com Hello\r\n",
            "250-PIPELINING\r\n",
            "250-STARTTLS\r\n",
            "250-CHUNKING\r\n",
            "250-ENHANCEDSTATUSCODES\r\n",
            "250 AUTH PLAIN\r\n",
        );
        let (_, ext, to_client, forwarded) = ehlo_relay(&config, false, cmd, rsp.as_bytes()).await;
        assert_eq!(forwarded, cmd);
        assert!(ext.allow_starttls(false));
        assert!(!ext.allow_chunking(&config));
        assert_eq!(
            to_client,
            concat!(
                "250-mail.example.com Hello\r\n",
                "250-PIPELINING\r\n",
                "250-STARTTLS\r\n",
                "250 AUTH PLAIN\r\n",
            )
            .as_bytes()
        );
    }

    #[tokio::test]
    async fn ehlo_extension_keywords_are_case_insensitive() {
        let config = SmtpInterceptionConfig::default();
        let cmd = b"ehlo client.example\r\n";
        let rsp = concat!(
            "250-mail.example.com Hello\r\n",
            "250-starttls\r\n",
            "250-chunking\r\n",
            "250-enhancedstatuscodes\r\n",
            "250 pipelining\r\n",
        );
        let (_, ext, to_client, _) = ehlo_relay(&config, false, cmd, rsp.as_bytes()).await;
        assert!(ext.allow_starttls(false));
        assert!(!ext.allow_chunking(&config));
        assert_eq!(
            to_client,
            concat!(
                "250-mail.example.com Hello\r\n",
                "250-starttls\r\n",
                "250 pipelining\r\n",
            )
            .as_bytes()
        );
    }

    #[tokio::test]
    async fn starttls_is_hidden_after_tls_upgrade() {
        let config = SmtpInterceptionConfig::default();
        let cmd = b"EHLO client.example\r\n";
        let rsp = concat!(
            "250-mail.example.com Hello\r\n",
            "250-STARTTLS\r\n",
            "250 PIPELINING\r\n",
        );
        let (_, ext, to_client, _) = ehlo_relay(&config, true, cmd, rsp.as_bytes()).await;
        assert!(!ext.allow_starttls(true));
        assert_eq!(
            to_client,
            concat!("250-mail.example.com Hello\r\n", "250 PIPELINING\r\n",).as_bytes()
        );
    }

    #[tokio::test]
    async fn chunking_is_advertised_when_allowed() {
        let config = SmtpInterceptionConfig {
            allow_data_chunking: true,
            ..SmtpInterceptionConfig::default()
        };
        let cmd = b"EHLO client.example\r\n";
        let rsp = concat!("250-mail.example.com Hello\r\n", "250 CHUNKING\r\n",);
        let (_, ext, to_client, _) = ehlo_relay(&config, false, cmd, rsp.as_bytes()).await;
        assert!(ext.allow_chunking(&config));
        assert_eq!(to_client, rsp.as_bytes());
    }

    #[tokio::test]
    async fn mail_before_ehlo_is_rejected_locally() {
        let config = SmtpInterceptionConfig::default();
        let mut initiation = Initiation::new(&config, local_ip(), false);
        let (mut clt_in, mut clt_r) = tokio::io::duplex(4096);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(4096);
        let (_ups_in, mut ups_r) = tokio::io::duplex(4096);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(4096);

        clt_in.write_all(b"MAIL FROM:<>\r\n").await.unwrap();
        drop(clt_in);

        let intercept = initiation.relay(&mut clt_r, &mut clt_w, &mut ups_r, &mut ups_w);
        let drive = async {
            let mut buf = [0u8; 128];
            let n = clt_out.read(&mut buf).await.unwrap();
            buf[..n].to_vec()
        };
        let (status, to_client) = tokio::join!(intercept, drive);
        assert!(matches!(status, Err(ServerTaskError::ClosedByClient)));
        assert_eq!(to_client, b"503 Bad sequence of commands\r\n");
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
    async fn ehlo_failure_allows_retry() {
        let config = SmtpInterceptionConfig::default();
        let mut initiation = Initiation::new(&config, local_ip(), false);
        let (mut clt_in, mut clt_r) = tokio::io::duplex(4096);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(4096);
        let (mut ups_in, mut ups_r) = tokio::io::duplex(4096);
        let (mut ups_w, mut ups_out) = tokio::io::duplex(4096);

        let first = b"EHLO client.example\r\n";
        let second = b"EHLO client.example\r\n";
        clt_in.write_all(first).await.unwrap();
        clt_in.write_all(second).await.unwrap();

        let intercept = initiation.relay(&mut clt_r, &mut clt_w, &mut ups_r, &mut ups_w);
        let drive = async {
            let mut buf = [0u8; 64];
            let n = ups_out.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], first);
            ups_in
                .write_all(b"550 5.7.1 Access denied\r\n")
                .await
                .unwrap();
            let n = clt_out.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], b"550 5.7.1 Access denied\r\n");
            let n = ups_out.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], second);
            ups_in
                .write_all(b"250 mail.example.com Hello\r\n")
                .await
                .unwrap();
        };
        let (status, _) = tokio::join!(intercept, drive);
        status.unwrap();
        let (host, _) = initiation.into_parts();
        assert_eq!(host.to_string(), "client.example");
    }
}
