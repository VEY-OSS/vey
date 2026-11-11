/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::IpAddr;
use std::time::Duration;

use anyhow::anyhow;
use tokio::io::{AsyncRead, AsyncWrite};

use vey_io_ext::{LineRecvBuf, RecvLineError};
use vey_smtp_proto::command::Command;
use vey_smtp_proto::response::{ResponseEncoder, ResponseParser};

use crate::serve::{ServerTaskError, ServerTaskResult};

pub(super) trait ResponseLineRecvExt {
    async fn read_rsp_line_with_feedback<'a, R, W>(
        &'a mut self,
        recv_timeout: Duration,
        ups_r: &mut R,
        clt_w: &mut W,
        local_ip: IpAddr,
    ) -> ServerTaskResult<&'a [u8]>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin;
}

impl<const MAX_LINE_SIZE: usize> ResponseLineRecvExt for LineRecvBuf<MAX_LINE_SIZE> {
    async fn read_rsp_line_with_feedback<'a, R, W>(
        &'a mut self,
        recv_timeout: Duration,
        ups_r: &mut R,
        clt_w: &mut W,
        local_ip: IpAddr,
    ) -> ServerTaskResult<&'a [u8]>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        match self.read_line_with_timeout(ups_r, recv_timeout).await {
            Ok(line) => Ok(line),
            Err(e) => match e {
                RecvLineError::IoError(e) => {
                    let _ = ResponseEncoder::upstream_io_error(local_ip, &e)
                        .write(clt_w)
                        .await;
                    Err(ServerTaskError::UpstreamReadFailed(e))
                }
                RecvLineError::IoClosed => {
                    let _ = ResponseEncoder::upstream_io_closed(local_ip)
                        .write(clt_w)
                        .await;
                    Err(ServerTaskError::ClosedByUpstream)
                }
                RecvLineError::Timeout => {
                    let _ = ResponseEncoder::local_service_not_available(local_ip)
                        .write(clt_w)
                        .await;
                    Err(ServerTaskError::UpstreamAppTimeout(
                        "timeout to get upstream response",
                    ))
                }
                RecvLineError::LineTooLong => {
                    let _ = ResponseEncoder::upstream_line_too_long(local_ip)
                        .write(clt_w)
                        .await;
                    Err(ServerTaskError::UpstreamAppError(anyhow!(
                        "SMTP response line too long"
                    )))
                }
            },
        }
    }
}

pub(super) trait ResponseParseExt {
    async fn feed_line_with_feedback<'a, W>(
        &mut self,
        line: &'a [u8],
        clt_w: &mut W,
        local_ip: IpAddr,
    ) -> ServerTaskResult<&'a [u8]>
    where
        W: AsyncWrite + Unpin;
}

impl ResponseParseExt for ResponseParser {
    async fn feed_line_with_feedback<'a, W>(
        &mut self,
        line: &'a [u8],
        clt_w: &mut W,
        local_ip: IpAddr,
    ) -> ServerTaskResult<&'a [u8]>
    where
        W: AsyncWrite + Unpin,
    {
        match self.feed_line(line) {
            Ok(msg) => Ok(msg),
            Err(e) => {
                let _ = ResponseEncoder::upstream_response_error(local_ip, &e)
                    .write(clt_w)
                    .await;
                Err(ServerTaskError::UpstreamAppError(anyhow!(
                    "invalid SMTP QUIT response line: {e}"
                )))
            }
        }
    }
}

pub(super) trait CommandLineRecvExt {
    async fn recv_cmd<CR, CW>(
        &mut self,
        recv_timeout: Duration,
        clt_r: &mut CR,
        clt_w: &mut CW,
    ) -> ServerTaskResult<(Command, &[u8])>
    where
        CR: AsyncRead + Unpin,
        CW: AsyncWrite + Unpin;

    async fn handle_line_error<CW>(e: RecvLineError, clt_w: &mut CW) -> ServerTaskError
    where
        CW: AsyncWrite + Unpin,
    {
        match e {
            RecvLineError::IoError(e) => ServerTaskError::ClientTcpReadFailed(e),
            RecvLineError::IoClosed => ServerTaskError::ClosedByClient,
            RecvLineError::Timeout => {
                ServerTaskError::ClientAppTimeout("timeout to wait client command")
            }
            RecvLineError::LineTooLong => {
                let _ = ResponseEncoder::COMMAND_LINE_TOO_LONG.write(clt_w).await;
                ServerTaskError::ClientAppError(anyhow!("SMTP command line too long"))
            }
        }
    }
}

impl<const MAX_LINE_SIZE: usize> CommandLineRecvExt for LineRecvBuf<MAX_LINE_SIZE> {
    async fn recv_cmd<CR, CW>(
        &mut self,
        recv_timeout: Duration,
        clt_r: &mut CR,
        clt_w: &mut CW,
    ) -> ServerTaskResult<(Command, &[u8])>
    where
        CR: AsyncRead + Unpin,
        CW: AsyncWrite + Unpin,
    {
        match self.read_line_with_timeout(clt_r, recv_timeout).await {
            Ok(line) => match Command::parse_line(line) {
                Ok(cmd) => Ok((cmd, line)),
                Err(e) => {
                    let _ = ResponseEncoder::from(&e).write(clt_w).await;
                    Err(ServerTaskError::ClientAppError(anyhow!(
                        "invalid SMTP command line: {e}"
                    )))
                }
            },
            Err(e) => {
                let e = Self::handle_line_error(e, clt_w).await;
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use vey_io_ext::LineRecvBuf;
    use vey_smtp_proto::command::Command;
    use vey_smtp_proto::response::ResponseParser;

    use super::*;

    #[tokio::test]
    async fn an_invalid_command_line_is_reported_to_the_client() {
        let mut buf = LineRecvBuf::<{ Command::MAX_LINE_SIZE }>::default();
        let (mut clt_in, mut clt_r) = tokio::io::duplex(256);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(256);
        clt_in.write_all(b"BDAT abc\r\n").await.unwrap();
        let status = buf
            .recv_cmd(Duration::from_secs(1), &mut clt_r, &mut clt_w)
            .await;
        assert!(matches!(status, Err(ServerTaskError::ClientAppError(_))));
        drop(clt_w);
        let mut out = Vec::new();
        clt_out.read_to_end(&mut out).await.unwrap();
        assert!(out.starts_with(b"500 "));
    }

    #[tokio::test]
    async fn a_closed_upstream_response_is_reported_to_the_client() {
        let mut buf = LineRecvBuf::<{ ResponseParser::MAX_LINE_SIZE }>::default();
        let (ups_in, mut ups_r) = tokio::io::duplex(256);
        let (mut clt_w, mut clt_out) = tokio::io::duplex(256);
        drop(ups_in);
        let status = buf
            .read_rsp_line_with_feedback(
                Duration::from_secs(1),
                &mut ups_r,
                &mut clt_w,
                "127.0.0.1".parse().unwrap(),
            )
            .await;
        assert!(matches!(status, Err(ServerTaskError::ClosedByUpstream)));
        drop(clt_w);
        let mut out = Vec::new();
        clt_out.read_to_end(&mut out).await.unwrap();
        assert!(out.windows(3).any(|w| w == b"554"));
    }

    #[tokio::test]
    async fn an_invalid_response_line_is_reported_to_the_client() {
        let mut parser = ResponseParser::default();
        let (mut clt_w, mut clt_out) = tokio::io::duplex(256);
        let status = parser
            .feed_line_with_feedback(b"not a reply\r\n", &mut clt_w, "127.0.0.1".parse().unwrap())
            .await;
        assert!(matches!(status, Err(ServerTaskError::UpstreamAppError(_))));
        drop(clt_w);
        let mut out = Vec::new();
        clt_out.read_to_end(&mut out).await.unwrap();
        assert!(out.windows(3).any(|w| w == b"554"));
    }
}
