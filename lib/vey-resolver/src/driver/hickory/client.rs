/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[cfg(feature = "quic")]
use anyhow::anyhow;
use async_recursion::async_recursion;
use hickory_net::client::{Client, ClientHandle};
use hickory_net::runtime::TokioRuntimeProvider;
use hickory_net::{BufDnsStreamHandle, DnsError, NetError};
use hickory_proto::rr::{DNSClass, Name, RData, Record, RecordType};
use rustls::ClientConfig;
use rustls_pki_types::ServerName;
use tokio::sync::mpsc;

use vey_socket::{BindAddr, TcpConnectInfo, UdpConnectInfo};
use vey_types::net::{
    DnsEncryptionConfig, DnsEncryptionProtocol, DomainName, TcpMiscSockOpts, UdpMiscSockOpts,
};

use crate::{ResolveDriverErrorReason, ResolveError, ResolvedRecord};

#[derive(Clone)]
pub(super) struct DnsRequest {
    domain: DomainName,
    rtype: RecordType,
}

impl DnsRequest {
    pub(super) fn query_ipv6(domain: DomainName) -> Self {
        DnsRequest {
            domain,
            rtype: RecordType::AAAA,
        }
    }

    pub(super) fn query_ipv4(domain: DomainName) -> Self {
        DnsRequest {
            domain,
            rtype: RecordType::A,
        }
    }
}

#[derive(Default)]
struct HickoryClientState {
    failed_count: AtomicUsize,
}

impl HickoryClientState {
    fn add_failed(&self) {
        self.failed_count.fetch_add(1, Ordering::Relaxed);
    }

    fn clear_failed(&self) -> usize {
        self.failed_count.swap(0, Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub(super) struct HickoryClient {
    config: Arc<HickoryClientConfig>,
    state: Arc<HickoryClientState>,
    client: Client<TokioRuntimeProvider>,
}

impl HickoryClient {
    pub(super) async fn new(config: HickoryClientConfig) -> anyhow::Result<Self> {
        let client = config.build_async_client().await?;
        Ok(HickoryClient {
            config: Arc::new(config),
            state: Arc::new(HickoryClientState::default()),
            client,
        })
    }

    pub(super) async fn run(
        mut self,
        req_receiver: kanal::AsyncReceiver<(DnsRequest, mpsc::Sender<ResolvedRecord>)>,
    ) {
        let (client_sender, mut client_receiver) = mpsc::channel(1);
        let mut check_interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            tokio::select! {
                biased;

                r = req_receiver.recv() => {
                    let Ok((req, rsp_sender)) = r else {
                        log::warn!(
                            "hickory dns client to {} stopping: request channel closed",
                            self.config.target
                        );
                        break;
                    };
                    let client_job = HickoryClientJob {
                        config: self.config.clone(),
                        state: self.state.clone(),
                        try_failed: self.config.each_tries,
                        try_truncated: self.config.retry_tcp(),
                    };
                    let async_client = self.client.clone();
                    tokio::spawn(async move {
                        let r = client_job.run(async_client, req, None).await;
                        let _ = rsp_sender.send(r).await;
                    });
                }
                _ = check_interval.tick() => {
                    if self.state.clear_failed() > 0 {
                        let client_sender = client_sender.clone();
                        let client_config = self.config.clone();
                        tokio::spawn(async move {
                            if let Ok(client) = client_config.build_async_client().await {
                                let _ = client_sender.send(client).await;
                            }
                        });
                    }
                }
                r = client_receiver.recv() => {
                    if let Some(client) = r {
                        self.client = client;
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct HickoryClientJob {
    config: Arc<HickoryClientConfig>,
    state: Arc<HickoryClientState>,
    try_failed: i32,
    try_truncated: bool,
}

impl HickoryClientJob {
    #[async_recursion]
    async fn run(
        mut self,
        mut async_client: Client<TokioRuntimeProvider>,
        req: DnsRequest,
        query_name: Option<Name>,
    ) -> ResolvedRecord {
        let mut name = match query_name {
            Some(name) => name,
            None => match Name::from_ascii(req.domain.as_fqdn_str()) {
                Ok(name) => name,
                Err(e) => {
                    return ResolvedRecord::failed(
                        req.domain,
                        self.config.negative_ttl,
                        ResolveDriverErrorReason::Owned(e.to_string()).into(),
                    );
                }
            },
        };
        // always use FQDN format such like "www.example.com."
        name.set_fqdn(true);

        match async_client
            .query(name.clone(), DNSClass::IN, req.rtype)
            .await
        {
            Ok(rsp) => {
                let (msg, _) = rsp.into_parts();
                let response_code = msg.response_code;
                if let Some(e) = ResolveError::from_response_code(response_code) {
                    return ResolvedRecord::failed(req.domain, self.config.negative_ttl, e);
                }

                if msg.truncation && self.try_truncated {
                    self.try_truncated = false;
                    if let Ok(client) = self.config.new_dns_over_tcp_client().await {
                        return self.run(client, req, Some(name)).await;
                    }
                }

                let (ips, ttl, has_cname, name) =
                    collect_answer_addresses(msg.answers, req.rtype, name);
                if ips.is_empty() {
                    if has_cname {
                        self.try_truncated = true;
                        self.run(async_client, req, Some(name)).await
                    } else {
                        ResolvedRecord::empty(req.domain, self.config.negative_ttl)
                    }
                } else {
                    ResolvedRecord::resolved(
                        req.domain,
                        ttl.unwrap_or(0),
                        self.config.positive_min_ttl,
                        self.config.positive_max_ttl,
                        ips,
                    )
                }
            }
            Err(NetError::Dns(e)) => match e {
                DnsError::ResponseCode(code) => {
                    let e = ResolveError::from_response_code(code).unwrap_or(
                        ResolveError::DriverError(ResolveDriverErrorReason::Static(
                            "hickory driver returned no-error response code as dns error",
                        )),
                    );
                    ResolvedRecord::failed(req.domain, self.config.negative_ttl, e)
                }
                DnsError::NoRecordsFound(v) => {
                    let ttl = v.negative_ttl.unwrap_or(self.config.negative_ttl);
                    if let Some(e) = ResolveError::from_response_code(v.response_code) {
                        ResolvedRecord::failed(req.domain, ttl, e)
                    } else {
                        ResolvedRecord::empty(req.domain, ttl)
                    }
                }
                _ => ResolvedRecord::failed(
                    req.domain,
                    self.config.negative_ttl,
                    ResolveError::DriverError(ResolveDriverErrorReason::Owned(e.to_string())),
                ),
            },
            Err(NetError::Proto(e)) => ResolvedRecord::failed(
                req.domain,
                self.config.negative_ttl,
                ResolveError::DriverError(ResolveDriverErrorReason::Owned(e.to_string())),
            ),
            Err(NetError::Timeout) => {
                self.state.add_failed();
                self.try_failed -= 1;
                if self.try_failed > 0
                    && let Ok(client) = self.config.build_async_client().await
                {
                    return self.run(client, req, Some(name)).await;
                }
                ResolvedRecord::failed(
                    req.domain,
                    self.config.negative_ttl,
                    ResolveError::DriverTimeout,
                )
            }
            Err(e) => {
                self.state.add_failed();
                self.try_failed -= 1;
                if self.try_failed > 0
                    && let Ok(client) = self.config.build_async_client().await
                {
                    return self.run(client, req, Some(name)).await;
                }
                ResolvedRecord::failed(
                    req.domain,
                    self.config.negative_ttl,
                    ResolveError::DriverError(ResolveDriverErrorReason::Owned(e.to_string())),
                )
            }
        }
    }
}

/// Collect A/AAAA addresses for `rtype` from the answer section.
///
/// TTL is the **minimum** among those address records only (not CNAME / other
/// RRtypes that may appear later in the answer section).
fn collect_answer_addresses(
    answers: impl IntoIterator<Item = Record>,
    rtype: RecordType,
    mut name: Name,
) -> (Vec<IpAddr>, Option<u32>, bool, Name) {
    let mut has_cname = false;
    let mut ips = Vec::with_capacity(4);
    let mut ttl = None;
    for r in answers {
        match r.data {
            RData::A(v) if rtype == RecordType::A => {
                ips.push(IpAddr::V4(v.0));
                ttl = Some(ttl.map_or(r.ttl, |t: u32| t.min(r.ttl)));
            }
            RData::AAAA(v) if rtype == RecordType::AAAA => {
                ips.push(IpAddr::V6(v.0));
                ttl = Some(ttl.map_or(r.ttl, |t: u32| t.min(r.ttl)));
            }
            RData::CNAME(v) if name.eq(&r.name) => {
                has_cname = true;
                name = v.0.clone();
            }
            _ => {}
        }
    }
    (ips, ttl, has_cname, name)
}

#[derive(Clone)]
pub(super) struct HickoryClientConfig {
    pub(super) target: SocketAddr,
    pub(super) bind: BindAddr,
    pub(super) encryption: Option<DnsEncryptionConfig>,
    pub(super) connect_timeout: Duration,
    pub(super) request_timeout: Duration,
    pub(super) each_tries: i32,
    pub(super) positive_min_ttl: u32,
    pub(super) positive_max_ttl: u32,
    pub(super) negative_ttl: u32,
    pub(super) tcp_misc_opts: TcpMiscSockOpts,
    pub(super) udp_misc_opts: UdpMiscSockOpts,
}

impl HickoryClientConfig {
    fn retry_tcp(&self) -> bool {
        self.encryption.is_none()
    }

    async fn build_async_client(&self) -> anyhow::Result<Client<TokioRuntimeProvider>> {
        if let Some(ec) = &self.encryption {
            let tls_client = ec.tls_client().driver.as_ref().clone();

            match ec.protocol() {
                DnsEncryptionProtocol::Tls => {
                    self.new_dns_over_tls_client(tls_client, ec.tls_name().clone())
                        .await
                }
                DnsEncryptionProtocol::Https => {
                    self.new_dns_over_h2_client(tls_client, ec.tls_name().clone())
                        .await
                }
                #[cfg(feature = "quic")]
                DnsEncryptionProtocol::Quic => {
                    self.new_dns_over_quic_client(tls_client, ec.tls_name())
                        .await
                }
                #[cfg(feature = "quic")]
                DnsEncryptionProtocol::H3 => {
                    self.new_dns_over_h3_client(tls_client, ec.tls_name()).await
                }
            }
        } else {
            self.new_dns_over_udp_client().await
        }
    }

    fn tcp_connect_info(&self) -> TcpConnectInfo {
        TcpConnectInfo {
            server: self.target,
            bind: self.bind,
            keepalive: Default::default(),
            misc_opts: self.tcp_misc_opts.clone(),
        }
    }

    fn udp_connect_info(&self) -> UdpConnectInfo {
        UdpConnectInfo {
            server: self.target,
            bind: self.bind,
            buf_conf: Default::default(),
            misc_opts: self.udp_misc_opts,
        }
    }

    async fn new_dns_over_udp_client(&self) -> anyhow::Result<Client<TokioRuntimeProvider>> {
        // random port is used here
        let client_connect =
            vey_hickory_client::io::udp::connect(self.udp_connect_info(), self.request_timeout)
                .await?;

        let (client, bg) = Client::from_sender(client_connect);
        tokio::spawn(bg);
        Ok(client)
    }

    async fn new_dns_over_tcp_client(&self) -> anyhow::Result<Client<TokioRuntimeProvider>> {
        let (message_sender, outbound_messages) = BufDnsStreamHandle::new(self.target);

        let tcp_connect = vey_hickory_client::io::tcp::connect(
            self.tcp_connect_info(),
            outbound_messages,
            self.connect_timeout,
        )
        .await?;

        let (client, bg) = Client::with_timeout(tcp_connect, message_sender, self.request_timeout);
        tokio::spawn(bg);
        Ok(client)
    }

    async fn new_dns_over_tls_client(
        &self,
        tls_client: ClientConfig,
        tls_name: ServerName<'static>,
    ) -> anyhow::Result<Client<TokioRuntimeProvider>> {
        let (message_sender, outbound_messages) = BufDnsStreamHandle::new(self.target);

        let tls_connect = vey_hickory_client::io::tls::connect(
            self.tcp_connect_info(),
            tls_client,
            tls_name,
            outbound_messages,
            self.connect_timeout,
        )
        .await?;

        let (client, bg) = Client::with_timeout(tls_connect, message_sender, self.request_timeout);
        tokio::spawn(bg);
        Ok(client)
    }

    async fn new_dns_over_h2_client(
        &self,
        tls_client: ClientConfig,
        tls_name: ServerName<'static>,
    ) -> anyhow::Result<Client<TokioRuntimeProvider>> {
        let client_connect = vey_hickory_client::io::h2::connect(
            self.tcp_connect_info(),
            tls_client,
            tls_name,
            self.connect_timeout,
            self.request_timeout,
        )
        .await?;

        let (client, bg) = Client::from_sender(client_connect);
        tokio::spawn(bg);
        Ok(client)
    }

    #[cfg(feature = "quic")]
    async fn new_dns_over_quic_client(
        &self,
        tls_client: ClientConfig,
        tls_name: &ServerName<'static>,
    ) -> anyhow::Result<Client<TokioRuntimeProvider>> {
        let tls_name = match tls_name {
            ServerName::DnsName(domain) => domain.as_ref().to_owned(),
            ServerName::IpAddress(ip) => IpAddr::from(*ip).to_string(),
            _ => return Err(anyhow!("unsupported tls server name: {tls_name:?}")),
        };

        let client_connect = vey_hickory_client::io::quic::connect(
            self.udp_connect_info(),
            tls_client,
            tls_name,
            self.connect_timeout,
            self.request_timeout,
        )
        .await?;

        let (client, bg) = Client::from_sender(client_connect);
        tokio::spawn(bg);
        Ok(client)
    }

    #[cfg(feature = "quic")]
    async fn new_dns_over_h3_client(
        &self,
        tls_client: ClientConfig,
        tls_name: &ServerName<'static>,
    ) -> anyhow::Result<Client<TokioRuntimeProvider>> {
        let tls_name = match tls_name {
            ServerName::DnsName(domain) => domain.as_ref().to_owned(),
            ServerName::IpAddress(ip) => IpAddr::from(*ip).to_string(),
            _ => return Err(anyhow!("unsupported tls server name type")),
        };

        let client_connect = vey_hickory_client::io::h3::connect(
            self.udp_connect_info(),
            tls_client,
            tls_name,
            self.connect_timeout,
            self.request_timeout,
        )
        .await?;

        let (client, bg) = Client::from_sender(client_connect);
        tokio::spawn(bg);
        Ok(client)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_proto::rr::rdata::{A, AAAA, CNAME};
    use std::net::{Ipv4Addr, Ipv6Addr};
    use std::str::FromStr;

    #[test]
    fn collect_answer_addresses_min_ttl_of_a_records() {
        let qname = Name::from_str("www.example.com.").unwrap();
        let answers = vec![
            Record::from_rdata(qname.clone(), 300, RData::A(A(Ipv4Addr::new(1, 2, 3, 4)))),
            Record::from_rdata(qname.clone(), 60, RData::A(A(Ipv4Addr::new(5, 6, 7, 8)))),
            Record::from_rdata(
                qname.clone(),
                120,
                RData::A(A(Ipv4Addr::new(9, 10, 11, 12))),
            ),
        ];

        let (ips, ttl, has_cname, _) = collect_answer_addresses(answers, RecordType::A, qname);
        assert_eq!(
            ips,
            vec![
                IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)),
                IpAddr::V4(Ipv4Addr::new(5, 6, 7, 8)),
                IpAddr::V4(Ipv4Addr::new(9, 10, 11, 12)),
            ]
        );
        assert_eq!(ttl, Some(60));
        assert!(!has_cname);
    }

    #[test]
    fn collect_answer_addresses_ignores_trailing_cname_ttl() {
        // Address records first, then a CNAME with a different TTL — old code
        // would overwrite ttl with the CNAME's value.
        let qname = Name::from_str("www.example.com.").unwrap();
        let cname = Name::from_str("cdn.example.net.").unwrap();
        let answers = vec![
            Record::from_rdata(qname.clone(), 60, RData::A(A(Ipv4Addr::new(1, 2, 3, 4)))),
            Record::from_rdata(qname.clone(), 3600, RData::CNAME(CNAME(cname.clone()))),
        ];

        let (ips, ttl, has_cname, next_name) =
            collect_answer_addresses(answers, RecordType::A, qname);
        assert_eq!(ips, vec![IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))]);
        assert_eq!(ttl, Some(60));
        assert!(has_cname);
        assert_eq!(next_name, cname);
    }

    #[test]
    fn collect_answer_addresses_ignores_unrelated_rrtype_ttl() {
        let qname = Name::from_str("www.example.com.").unwrap();
        let answers = vec![
            Record::from_rdata(qname.clone(), 90, RData::A(A(Ipv4Addr::new(1, 2, 3, 4)))),
            // AAAA while querying A — must not affect TTL
            Record::from_rdata(qname.clone(), 10, RData::AAAA(AAAA(Ipv6Addr::LOCALHOST))),
        ];

        let (ips, ttl, has_cname, _) = collect_answer_addresses(answers, RecordType::A, qname);
        assert_eq!(ips, vec![IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))]);
        assert_eq!(ttl, Some(90));
        assert!(!has_cname);
    }

    #[test]
    fn collect_answer_addresses_aaaa_min_ttl() {
        let qname = Name::from_str("www.example.com.").unwrap();
        let answers = vec![
            Record::from_rdata(
                qname.clone(),
                200,
                RData::AAAA(AAAA(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1))),
            ),
            Record::from_rdata(
                qname.clone(),
                50,
                RData::AAAA(AAAA(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 2))),
            ),
        ];

        let (ips, ttl, _, _) = collect_answer_addresses(answers, RecordType::AAAA, qname);
        assert_eq!(ips.len(), 2);
        assert_eq!(ttl, Some(50));
    }

    #[test]
    fn collect_answer_addresses_cname_only() {
        let qname = Name::from_str("www.example.com.").unwrap();
        let cname = Name::from_str("cdn.example.net.").unwrap();
        let answers = vec![Record::from_rdata(
            qname.clone(),
            3600,
            RData::CNAME(CNAME(cname.clone())),
        )];

        let (ips, ttl, has_cname, next_name) =
            collect_answer_addresses(answers, RecordType::A, qname);
        assert!(ips.is_empty());
        assert_eq!(ttl, None);
        assert!(has_cname);
        assert_eq!(next_name, cname);
    }
}
