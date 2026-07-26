/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::time::Duration;

use openssl::x509::X509;

use vey_io_ext::EffectiveCacheHandle;
use vey_types::net::{Host, TlsCertUsage, TlsServiceType};

use super::{CacheQueryKey, FakeCertPair};

pub struct CertAgentHandle {
    inner: EffectiveCacheHandle<CacheQueryKey, FakeCertPair>,
    request_timeout: Duration,
}

impl CertAgentHandle {
    pub(super) fn new(
        inner: EffectiveCacheHandle<CacheQueryKey, FakeCertPair>,
        request_timeout: Duration,
    ) -> Self {
        CertAgentHandle {
            inner,
            request_timeout,
        }
    }

    pub async fn pre_fetch(
        &self,
        service: TlsServiceType,
        usage: TlsCertUsage,
        host: Host,
    ) -> Option<FakeCertPair> {
        let query_key = CacheQueryKey::new(service, usage, host);
        self.inner
            .fetch_cache_only(Arc::new(query_key), self.request_timeout)
            .await
            .and_then(|r| r.inner().cloned())
    }

    pub async fn fetch(
        &self,
        service: TlsServiceType,
        usage: TlsCertUsage,
        host: Host,
        mimic_cert: X509,
    ) -> Option<FakeCertPair> {
        let mut query_key = CacheQueryKey::new(service, usage, host);
        query_key.set_mimic_cert(mimic_cert);
        self.inner
            .fetch(Arc::new(query_key), self.request_timeout)
            .await
            .and_then(|r| r.inner().cloned())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::time::Duration;

    use tokio::net::UdpSocket;

    use vey_types::net::{Host, TlsCertUsage, TlsServiceType};

    use super::super::{CertAgentConfig, Request};
    use crate::test_util;

    #[tokio::test]
    async fn fetch_from_mock_udp_generator() {
        let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let peer_addr = peer.local_addr().unwrap();
        let (_, _, pem, der_key) = test_util::self_signed_cert_key();
        let (mimic_cert, _, _, _) = test_util::self_signed_cert_key();

        tokio::spawn(async move {
            let mut buf = [0u8; 16384];
            let (n, from) = peer.recv_from(&mut buf).await.unwrap();
            let req = Request::parse_req(&buf[..n]).unwrap();
            assert_eq!(req.host(), &Host::from_str("fetch.example").unwrap());
            assert!(req.cert().is_some());
            let rsp = req.encode_rsp(&pem, &der_key, 90).unwrap();
            peer.send_to(&rsp, from).await.unwrap();
        });

        let mut config = CertAgentConfig::default();
        config.set_query_peer_addr(peer_addr);
        config.set_cache_request_timeout(Duration::from_secs(2));
        config.set_query_wait_timeout(Duration::from_secs(2));
        let handle = config.spawn_cert_agent().unwrap();

        let pair = handle
            .fetch(
                TlsServiceType::Http,
                TlsCertUsage::TlsServer,
                Host::from_str("fetch.example").unwrap(),
                mimic_cert,
            )
            .await;
        assert!(pair.is_some());
        assert_eq!(pair.unwrap().certs.len(), 1);

        // Cache hit without querying again
        let cached = handle
            .pre_fetch(
                TlsServiceType::Http,
                TlsCertUsage::TlsServer,
                Host::from_str("fetch.example").unwrap(),
            )
            .await;
        assert!(cached.is_some());
    }
}
