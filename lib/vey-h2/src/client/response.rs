/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 VEY-OSS developers.
 */

use std::future::poll_fn;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use h2::client::ResponseFuture;
use h2::{Error, RecvStream};
use http::Response;

pub struct H2ResponseHeaderReceiver {
    rsp_fut: ResponseFuture,
    rsp_body: Option<RecvStream>,
}

impl H2ResponseHeaderReceiver {
    pub fn new(rsp_fut: ResponseFuture) -> Self {
        Self {
            rsp_fut,
            rsp_body: None,
        }
    }

    fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Result<Response<()>, Error>> {
        if let Some(r) = ready!(self.rsp_fut.poll_informational(cx)) {
            return Poll::Ready(r);
        }

        let r = ready!(Pin::new(&mut self.rsp_fut).poll(cx))?;
        let (header, body) = r.into_parts();

        self.rsp_body = Some(body);
        Poll::Ready(Ok(Response::from_parts(header, ())))
    }

    pub async fn recv_header(&mut self) -> Result<Response<()>, Error> {
        poll_fn(move |cx| self.poll_recv(cx)).await
    }

    #[inline]
    pub fn take_body(&mut self) -> Option<RecvStream> {
        self.rsp_body.take()
    }
}
