/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::{self, IoSlice};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use http::Method;
use pin_project_lite::pin_project;
use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite, ReadBuf};

use vey_http::client::{HttpForwardRemoteResponse, HttpResponseParseError};
use vey_http::server::HttpProxyClientRequest;
use vey_io_ext::{LimitedBufReader, LimitedWriter, NilLimitedStats};
use vey_types::net::UpstreamAddr;

use super::{HttpForwardRead, HttpForwardWrite, send_req_header_to_origin};
use crate::auth::UserUpstreamTrafficStatsList;
use crate::module::http_forward::{
    ArcHttpForwardTaskRemoteStats, HttpForwardTaskNotes, HttpForwardTaskRemoteWrapperStats,
};
use crate::serve::ServerTaskNotes;

pin_project! {
    /// Origin-form HTTP/1 writer over TLS plaintext. Task + user stats only.
    pub(crate) struct TlsHttpForwardWriter {
        #[pin]
        inner: LimitedWriter<Box<dyn AsyncWrite + Unpin + Send + Sync>>,
    }
}

impl TlsHttpForwardWriter {
    pub(crate) fn new(
        ups_w: Box<dyn AsyncWrite + Unpin + Send + Sync>,
        stats: Arc<HttpForwardTaskRemoteWrapperStats>,
    ) -> Self {
        TlsHttpForwardWriter {
            inner: LimitedWriter::new(ups_w, stats),
        }
    }
}

impl AsyncWrite for TlsHttpForwardWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.project().inner.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_shutdown(cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.project().inner.poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}

#[async_trait]
impl HttpForwardWrite for TlsHttpForwardWriter {
    fn prepare_new(&mut self, _task_notes: &ServerTaskNotes, _upstream: &UpstreamAddr) {}

    fn update_stats(
        &mut self,
        task_stats: &ArcHttpForwardTaskRemoteStats,
        user_stats: UserUpstreamTrafficStatsList,
    ) {
        let mut wrapper_stats = HttpForwardTaskRemoteWrapperStats::new(Arc::clone(task_stats));
        wrapper_stats.push_user_io_stats(user_stats);
        self.inner.reset_stats(Arc::new(wrapper_stats));
    }

    async fn send_request_header(
        &mut self,
        req: &HttpProxyClientRequest,
        body: Option<&[u8]>,
    ) -> io::Result<()> {
        send_req_header_to_origin(&mut self.inner, req, body).await
    }
}

pin_project! {
    /// Origin-form HTTP/1 reader over TLS plaintext. Task + user stats only.
    pub(crate) struct TlsHttpForwardReader {
        #[pin]
        inner: LimitedBufReader<Box<dyn AsyncRead + Unpin + Send + Sync>>,
    }
}

impl TlsHttpForwardReader {
    pub(crate) fn new(
        ups_r: Box<dyn AsyncRead + Unpin + Send + Sync>,
        stats: Arc<HttpForwardTaskRemoteWrapperStats>,
    ) -> Self {
        TlsHttpForwardReader {
            inner: LimitedBufReader::new_unlimited(
                ups_r,
                Arc::new(NilLimitedStats::default()),
                stats,
            ),
        }
    }
}

impl AsyncRead for TlsHttpForwardReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.project().inner.poll_read(cx, buf)
    }
}

impl AsyncBufRead for TlsHttpForwardReader {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        self.project().inner.poll_fill_buf(cx)
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        self.project().inner.consume(amt)
    }
}

#[async_trait]
impl HttpForwardRead for TlsHttpForwardReader {
    fn update_stats(
        &mut self,
        task_stats: &ArcHttpForwardTaskRemoteStats,
        user_stats: UserUpstreamTrafficStatsList,
    ) {
        let mut wrapper_stats = HttpForwardTaskRemoteWrapperStats::new(Arc::clone(task_stats));
        wrapper_stats.push_user_io_stats(user_stats);
        self.inner.reset_buffer_stats(Arc::new(wrapper_stats));
    }

    async fn recv_response_header(
        &mut self,
        method: &Method,
        keep_alive: bool,
        max_header_size: usize,
        http_notes: &mut HttpForwardTaskNotes,
    ) -> Result<HttpForwardRemoteResponse, HttpResponseParseError> {
        let rsp =
            HttpForwardRemoteResponse::parse(&mut self.inner, method, keep_alive, max_header_size)
                .await?;
        http_notes.rsp_status = rsp.code;
        http_notes.origin_status = rsp.code;
        Ok(rsp)
    }
}
