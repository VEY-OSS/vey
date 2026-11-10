/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use std::net::IpAddr;
use std::time::Duration;

use anyhow::anyhow;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use vey_io_ext::{LimitedWriteExt, LineRecvBuf, RecvLineError};
use vey_smtp_proto::command::Command;
use vey_smtp_proto::response::{ReplyCode, ResponseEncoder, ResponseParser};

use crate::serve::{ServerTaskError, ServerTaskResult};

pub(super) struct EndQuitServer {}

impl EndQuitServer {
    pub(super) async fn run_to_end<R, W>(
        ups_r: R,
        mut ups_w: W,
        timeout: Duration,
    ) -> ServerTaskResult<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        ups_w
            .write_all_flush(b"QUIT\r\n")
            .await
            .map_err(ServerTaskError::UpstreamWriteFailed)?;

        tokio::time::timeout(timeout, EndQuitServer::wait_quit_reply(ups_r))
            .await
            .map_err(|_| {
                ServerTaskError::UpstreamAppTimeout("timeout to wait SMTP QUIT response")
            })?
    }

    async fn wait_quit_reply<R>(mut ups_r: R) -> ServerTaskResult<()>
    where
        R: AsyncRead + Unpin,
    {
        let mut recv_buf = LineRecvBuf::<{ ResponseParser::MAX_LINE_SIZE }>::default();

        let mut rsp = ResponseParser::default();
        loop {
            let line = recv_buf.read_line(&mut ups_r).await.map_err(|e| match e {
                RecvLineError::IoError(e) => ServerTaskError::UpstreamReadFailed(e),
                RecvLineError::IoClosed => ServerTaskError::ClosedByUpstream,
                RecvLineError::Timeout => {
                    ServerTaskError::UpstreamAppTimeout("timeout to get upstream response")
                }
                RecvLineError::LineTooLong => {
                    ServerTaskError::UpstreamAppError(anyhow!("SMTP response line too long"))
                }
            })?;

            rsp.feed_line(line).map_err(|e| {
                ServerTaskError::UpstreamAppError(anyhow!("invalid SMTP QUIT response line: {e}"))
            })?;
            if rsp.code() != ReplyCode::SERVICE_CLOSING {
                return Err(ServerTaskError::UpstreamAppError(anyhow!(
                    "invalid SMTP QUIT response code: {}",
                    rsp.code()
                )));
            }
            if rsp.finished() {
                break;
            }

            recv_buf.consume_line();
        }

        Ok(())
    }
}

pub(super) struct EndWaitClient {
    local_ip: IpAddr,
}

impl EndWaitClient {
    pub(super) fn new(local_ip: IpAddr) -> Self {
        EndWaitClient { local_ip }
    }

    pub(super) async fn run_to_end<R, W>(
        self,
        clt_r: R,
        clt_w: W,
        timeout: Duration,
    ) -> ServerTaskResult<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        tokio::time::timeout(timeout, self.wait_quit_command(clt_r, clt_w))
            .await
            .map_err(|_| {
                ServerTaskError::ClientAppError(anyhow!("timeout to wait SMTP QUIT command"))
            })?
    }

    async fn wait_quit_command<R, W>(self, mut clt_r: R, mut clt_w: W) -> ServerTaskResult<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut recv_buf = LineRecvBuf::<{ Command::MAX_LINE_SIZE }>::default();
        loop {
            let line = match recv_buf.read_line(&mut clt_r).await {
                Ok(line) => line,
                Err(e) => {
                    let e = match e {
                        RecvLineError::IoError(e) => ServerTaskError::ClientTcpReadFailed(e),
                        RecvLineError::IoClosed => ServerTaskError::ClosedByClient,
                        RecvLineError::Timeout => {
                            ServerTaskError::ClientAppTimeout("timeout to wait client command")
                        }
                        RecvLineError::LineTooLong => {
                            let _ = ResponseEncoder::COMMAND_LINE_TOO_LONG
                                .write(&mut clt_w)
                                .await;
                            ServerTaskError::ClientAppError(anyhow!("SMTP command line too long"))
                        }
                    };
                    return Err(e);
                }
            };

            let cmd = match Command::parse_line(line) {
                Ok(cmd) => {
                    recv_buf.consume_line();
                    cmd
                }
                Err(e) => {
                    let _ = ResponseEncoder::from(&e).write(&mut clt_w).await;
                    return Err(ServerTaskError::ClientAppError(anyhow!(
                        "invalid SMTP command line: {e}"
                    )));
                }
            };
            if cmd == Command::Quit {
                ResponseEncoder::local_service_closing(self.local_ip)
                    .write(&mut clt_w)
                    .await
                    .map_err(ServerTaskError::ClientTcpWriteFailed)?;

                break;
            } else {
                ResponseEncoder::BAD_SEQUENCE_OF_COMMANDS
                    .write(&mut clt_w)
                    .await
                    .map_err(ServerTaskError::ClientTcpWriteFailed)?;
            }
        }

        let _ = clt_w.shutdown().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn quit_server_accepts_a_closing_reply() {
        let (mut ups_in, ups_r) = tokio::io::duplex(1024);
        let (ups_w, mut ups_out) = tokio::io::duplex(1024);
        let run = EndQuitServer::run_to_end(ups_r, ups_w, Duration::from_secs(1));
        let drive = async {
            let mut buf = [0u8; 16];
            let n = ups_out.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], b"QUIT\r\n");
            ups_in.write_all(b"221 Bye\r\n").await.unwrap();
        };
        let (status, _) = tokio::join!(run, drive);
        status.unwrap();
    }

    #[tokio::test]
    async fn quit_server_accepts_a_multiline_closing_reply() {
        let (mut ups_in, ups_r) = tokio::io::duplex(1024);
        let (ups_w, mut ups_out) = tokio::io::duplex(1024);
        let run = EndQuitServer::run_to_end(ups_r, ups_w, Duration::from_secs(1));
        let drive = async {
            let mut buf = [0u8; 16];
            let n = ups_out.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], b"QUIT\r\n");
            ups_in
                .write_all(b"221-closing\r\n221 Bye\r\n")
                .await
                .unwrap();
        };
        let (status, _) = tokio::join!(run, drive);
        status.unwrap();
    }

    #[tokio::test]
    async fn quit_server_rejects_a_non_closing_reply() {
        let (mut ups_in, ups_r) = tokio::io::duplex(1024);
        let (ups_w, mut ups_out) = tokio::io::duplex(1024);
        let run = EndQuitServer::run_to_end(ups_r, ups_w, Duration::from_secs(1));
        let drive = async {
            let mut buf = [0u8; 16];
            let _ = ups_out.read(&mut buf).await.unwrap();
            ups_in.write_all(b"250 OK\r\n").await.unwrap();
        };
        let (status, _) = tokio::join!(run, drive);
        assert!(matches!(status, Err(ServerTaskError::UpstreamAppError(_))));
    }

    #[tokio::test]
    async fn quit_server_times_out_without_a_reply() {
        let (_ups_in, ups_r) = tokio::io::duplex(1024);
        let (ups_w, mut ups_out) = tokio::io::duplex(1024);
        let run = EndQuitServer::run_to_end(ups_r, ups_w, Duration::from_millis(20));
        let drive = async {
            let mut buf = [0u8; 16];
            let _ = ups_out.read(&mut buf).await.unwrap();
        };
        let (status, _) = tokio::join!(run, drive);
        assert!(matches!(
            status,
            Err(ServerTaskError::UpstreamAppTimeout(_))
        ));
    }

    #[tokio::test]
    async fn wait_client_rejects_other_commands_until_quit() {
        let end = EndWaitClient::new("127.0.0.1".parse().unwrap());
        let (mut clt_in, clt_r) = tokio::io::duplex(1024);
        let (clt_w, mut clt_out) = tokio::io::duplex(1024);
        clt_in.write_all(b"NOOP\r\nQUIT\r\n").await.unwrap();
        end.run_to_end(clt_r, clt_w, Duration::from_secs(1))
            .await
            .unwrap();
        let mut out = Vec::new();
        clt_out.read_to_end(&mut out).await.unwrap();
        assert_eq!(
            out,
            concat!(
                "503 Bad sequence of commands\r\n",
                "221 [127.0.0.1] Service closing transmission channel\r\n",
            )
            .as_bytes()
        );
    }

    #[tokio::test]
    async fn wait_client_rejects_an_invalid_command_line() {
        let end = EndWaitClient::new("127.0.0.1".parse().unwrap());
        let (mut clt_in, clt_r) = tokio::io::duplex(1024);
        let (clt_w, mut clt_out) = tokio::io::duplex(1024);
        clt_in.write_all(b"BDAT abc\r\n").await.unwrap();
        drop(clt_in);
        let status = end.run_to_end(clt_r, clt_w, Duration::from_secs(1)).await;
        assert!(matches!(status, Err(ServerTaskError::ClientAppError(_))));
        let mut out = Vec::new();
        clt_out.read_to_end(&mut out).await.unwrap();
        assert!(out.starts_with(b"500 "));
    }
}
