/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use bytes::BufMut;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use super::{IcapOptionsParseError, IcapServiceOptions};
use crate::{IcapClientConnection, IcapServiceConfig};

pub(crate) struct IcapOptionsRequest<'a> {
    config: &'a IcapServiceConfig,
}

impl<'a> IcapOptionsRequest<'a> {
    pub(crate) fn new(config: &'a IcapServiceConfig) -> Self {
        IcapOptionsRequest { config }
    }

    fn build_header(&self) -> Vec<u8> {
        let mut header = self.config.build_options_request();
        if self.config.icap_206_enable {
            header.put_slice(b"Allow: 204, 206\r\n");
        } else {
            header.put_slice(b"Allow: 204\r\n");
        }
        // RFC 3507 §4.4.1: Encapsulated MUST be included in every ICAP message
        header.put_slice(b"Encapsulated: null-body=0\r\n");
        header.put_slice(b"\r\n");
        header
    }

    async fn send<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        writer.write_all(&self.build_header()).await
    }

    pub(crate) async fn get_options(
        &self,
        conn: &mut IcapClientConnection,
        max_header_size: usize,
    ) -> Result<IcapServiceOptions, IcapOptionsParseError> {
        self.send(&mut conn.writer)
            .await
            .map_err(IcapOptionsParseError::IoFailed)?;
        conn.mark_writer_finished();

        let mut options =
            IcapServiceOptions::parse(&mut conn.reader, self.config.method, max_header_size)
                .await?;
        conn.mark_reader_finished();

        if !self.config.icap_206_enable {
            options.support_206 = false;
        }
        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IcapMethod;
    use url::Url;

    #[test]
    fn options_request_includes_encapsulated_null_body() {
        let url = Url::parse("icap://icap.example/reqmod").unwrap();
        let config = IcapServiceConfig::new(IcapMethod::Reqmod, url).unwrap();
        let req = IcapOptionsRequest::new(&config);
        let text = String::from_utf8(req.build_header()).unwrap();

        assert!(text.starts_with("OPTIONS icap://icap.example/reqmod ICAP/1.0\r\n"));
        assert!(text.contains("Allow: 204\r\n"));
        assert!(text.contains("Encapsulated: null-body=0\r\n"));
        assert!(text.ends_with("\r\n\r\n"));
    }

    #[test]
    fn options_request_allow_includes_206_when_enabled() {
        let url = Url::parse("icap://icap.example/reqmod").unwrap();
        let mut config = IcapServiceConfig::new(IcapMethod::Reqmod, url).unwrap();
        config.icap_206_enable = true;
        let req = IcapOptionsRequest::new(&config);
        let text = String::from_utf8(req.build_header()).unwrap();

        assert!(text.contains("Allow: 204, 206\r\n"));
        assert!(text.contains("Encapsulated: null-body=0\r\n"));
    }
}
