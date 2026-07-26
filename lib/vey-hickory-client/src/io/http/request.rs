/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::FromStr;

use anyhow::anyhow;
use http::uri::{Authority, Parts, PathAndQuery, Scheme};
use http::{HeaderValue, Method, Request, Uri, Version, header};

pub struct HttpDnsRequestBuilder {
    pre_built_req: Request<()>,
}

impl HttpDnsRequestBuilder {
    pub fn new(version: Version, host: &str) -> anyhow::Result<Self> {
        let mut parts = Parts::default();
        parts.scheme = Some(Scheme::HTTPS);
        parts.authority =
            Some(Authority::from_str(host).map_err(|e| anyhow!("invalid authority: {e}"))?);
        parts.path_and_query = Some(PathAndQuery::from_static(super::DNS_QUERY_PATH));

        let url = Uri::from_parts(parts).map_err(|e| anyhow!("invalid url: {e}"))?;

        let request = Request::builder()
            .method(Method::POST)
            .uri(url)
            .version(version)
            .header(header::CONTENT_TYPE, super::MIME_APPLICATION_DNS)
            .header(header::ACCEPT, super::MIME_APPLICATION_DNS)
            .body(())
            .map_err(|e| anyhow!("failed to build http request header: {e}"))?;

        Ok(HttpDnsRequestBuilder {
            pre_built_req: request,
        })
    }

    pub fn post(&self, content_length: usize) -> Request<()> {
        let mut req = self.pre_built_req.clone();
        req.headers_mut()
            .insert(header::CONTENT_LENGTH, HeaderValue::from(content_length));
        req
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::header;

    #[test]
    fn new_builds_post_dns_query_request() {
        let builder = HttpDnsRequestBuilder::new(Version::HTTP_11, "dns.example.com:443").unwrap();
        let req = builder.post(512);

        assert_eq!(req.method(), Method::POST);
        assert_eq!(req.version(), Version::HTTP_11);
        assert_eq!(req.uri().scheme().unwrap().as_str(), "https");
        assert_eq!(req.uri().authority().unwrap().as_str(), "dns.example.com:443");
        assert_eq!(req.uri().path(), "/dns-query");
        assert_eq!(
            req.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/dns-message"
        );
        assert_eq!(
            req.headers().get(header::ACCEPT).unwrap(),
            "application/dns-message"
        );
        assert_eq!(req.headers().get(header::CONTENT_LENGTH).unwrap(), "512");
    }

    #[test]
    fn new_rejects_invalid_authority() {
        match HttpDnsRequestBuilder::new(Version::HTTP_11, "bad authority") {
            Err(e) => assert!(e.to_string().contains("invalid authority")),
            Ok(_) => panic!("expected invalid authority error"),
        }
    }

    #[test]
    fn new_builds_http2_dns_query_request() {
        let builder = HttpDnsRequestBuilder::new(Version::HTTP_2, "dns.google").unwrap();
        let req = builder.post(128);

        assert_eq!(req.version(), Version::HTTP_2);
        assert_eq!(req.uri().authority().unwrap().host(), "dns.google");
        assert_eq!(req.headers().get(header::CONTENT_LENGTH).unwrap(), "128");
    }
}
