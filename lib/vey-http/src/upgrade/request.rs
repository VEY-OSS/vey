/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::{self, Write};

use tokio::io::{AsyncWrite, AsyncWriteExt};

use vey_types::net::UpstreamAddr;

/// the extra header lines should end with \r\n
pub struct HttpUpgradeRequest<'a> {
    host: &'a UpstreamAddr,
    static_headers: &'a [String],
    dyn_headers: Vec<String>,
}

impl<'a> HttpUpgradeRequest<'a> {
    pub fn new(host: &'a UpstreamAddr, static_headers: &'a [String]) -> Self {
        HttpUpgradeRequest {
            host,
            static_headers,
            dyn_headers: Vec::new(),
        }
    }

    pub fn append_dyn_header(&mut self, line: String) {
        debug_assert!(line.ends_with("\r\n"));
        self.dyn_headers.push(line);
    }

    pub async fn send_connect_udp<W>(&self, target: &UpstreamAddr, writer: &mut W) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut buf = Vec::with_capacity(256);
        write!(
            &mut buf,
            "GET /.well-known/masque/udp/{}/{} HTTP/1.1\r\nHost: {}\r\n",
            target.host(),
            target.port(),
            self.host,
        )?;
        buf.extend_from_slice(b"Connection: Upgrade\r\n");
        buf.extend_from_slice(b"Upgrade: connect-udp\r\n");
        buf.extend_from_slice(b"Capsule-Protocol: ?1\r\n");
        for line in self.static_headers {
            debug_assert!(line.ends_with("\r\n"));
            buf.extend_from_slice(line.as_bytes());
        }
        for line in &self.dyn_headers {
            buf.extend_from_slice(line.as_bytes());
        }
        buf.extend_from_slice(b"\r\n");
        writer.write_all(&buf).await?;
        writer.flush().await
    }
}
