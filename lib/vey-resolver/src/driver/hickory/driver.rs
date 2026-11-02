/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::Instant;

use vey_types::net::DomainName;

use super::DnsRequest;
use crate::config::ResolverRuntimeConfig;
use crate::message::ResolveDriverResponse;
use crate::{ResolveDriver, ResolveDriverErrorReason, ResolveError, ResolvedRecord};

#[derive(Clone)]
pub struct HickoryResolver {
    each_timeout: Duration,
    retry_interval: Duration,
    negative_min_ttl: u32,
    clients: Vec<kanal::AsyncSender<(DnsRequest, mpsc::Sender<ResolvedRecord>)>>,
}

impl ResolveDriver for HickoryResolver {
    fn query_v4(
        &self,
        domain: DomainName,
        config: &ResolverRuntimeConfig,
        sender: mpsc::UnboundedSender<ResolveDriverResponse>,
    ) {
        let request = DnsRequest::query_ipv4(domain.clone());

        let job = self.clone();
        let timeout = config.protective_query_timeout;
        tokio::spawn(async move {
            let r = run_timed(job, timeout, domain, request).await;
            let _ = sender.send(ResolveDriverResponse::V4(r));
        });
    }

    fn query_v6(
        &self,
        domain: DomainName,
        config: &ResolverRuntimeConfig,
        sender: mpsc::UnboundedSender<ResolveDriverResponse>,
    ) {
        let request = DnsRequest::query_ipv6(domain.clone());

        let job = self.clone();
        let timeout = config.protective_query_timeout;
        tokio::spawn(async move {
            let r = run_timed(job, timeout, domain, request).await;
            let _ = sender.send(ResolveDriverResponse::V6(r));
        });
    }
}

async fn run_timed(
    job: HickoryResolver,
    timeout: Duration,
    domain: DomainName,
    request: DnsRequest,
) -> ResolvedRecord {
    let error_ttl = job.negative_min_ttl;
    match tokio::time::timeout(timeout, job.run(domain.clone(), request)).await {
        Ok(r) => r,
        Err(_) => ResolvedRecord::timed_out(domain, error_ttl),
    }
}

impl HickoryResolver {
    pub(super) fn new(
        each_timeout: Duration,
        retry_interval: Duration,
        negative_min_ttl: u32,
    ) -> Self {
        HickoryResolver {
            each_timeout,
            retry_interval,
            negative_min_ttl,
            clients: Vec::with_capacity(2),
        }
    }

    pub(super) fn push_client(
        &mut self,
        req_sender: kanal::AsyncSender<(DnsRequest, mpsc::Sender<ResolvedRecord>)>,
    ) {
        self.clients.push(req_sender);
    }

    async fn run(self, domain: DomainName, request: DnsRequest) -> ResolvedRecord {
        let (rsp_sender, mut rsp_receiver) = mpsc::channel::<ResolvedRecord>(1);

        let mut wait_left = self.clients.len();
        let mut clients = self.clients.into_iter();
        let Some(client) = clients.next() else {
            return ResolvedRecord::failed(
                domain,
                self.negative_min_ttl,
                ResolveError::NoResolverRunning,
            );
        };
        if client
            .send((request.clone(), rsp_sender.clone()))
            .await
            .is_err()
        {
            wait_left -= 1;
        }

        let mut last_err: Option<ResolvedRecord> = None;
        let mut interval =
            tokio::time::interval_at(Instant::now() + self.retry_interval, self.retry_interval);
        loop {
            tokio::select! {
                biased;

                r = rsp_receiver.recv() => {
                    wait_left -= 1;
                    match r {
                        Some(v) => {
                            if v.is_ok() || wait_left == 0 {
                                return v;
                            }
                            last_err = Some(v);
                        }
                        None => unreachable!(), // as we keep a rsp_sender here
                    }
                }
                _ = interval.tick() => {
                    if let Some(client) = clients.next() {
                        if client.try_send((request.clone(), rsp_sender.clone())).is_err() {
                            wait_left -= 1;
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        drop(rsp_sender);
        if wait_left == 0 {
            return last_err.unwrap_or_else(|| {
                ResolvedRecord::failed(
                    domain,
                    self.negative_min_ttl,
                    ResolveDriverErrorReason::Static("no response received").into(),
                )
            });
        }

        let mut channel_closed = false;
        if let Some(d) = self.each_timeout.checked_sub(self.retry_interval) {
            let deadline = Instant::now() + d;
            while wait_left > 0 {
                let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                    break;
                };
                match tokio::time::timeout(left, rsp_receiver.recv()).await {
                    Ok(Some(v)) => {
                        wait_left -= 1;
                        if v.is_ok() || wait_left == 0 {
                            return v;
                        }
                        last_err = Some(v);
                    }
                    Ok(None) => {
                        channel_closed = true;
                        break;
                    }
                    Err(_) => break,
                }
            }
        }

        last_err.unwrap_or_else(|| {
            let reason = if channel_closed {
                ResolveDriverErrorReason::Static("no response received").into()
            } else {
                ResolveError::DriverTimeout
            };
            ResolvedRecord::failed(domain, self.negative_min_ttl, reason)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::str::FromStr;
    use std::time::Duration;

    use tokio::time::Instant;
    use vey_types::net::DomainName;

    use super::*;

    fn domain() -> DomainName {
        DomainName::from_str("example.com").unwrap()
    }

    fn spawn_server(
        delay: Duration,
        record: ResolvedRecord,
    ) -> kanal::AsyncSender<(DnsRequest, mpsc::Sender<ResolvedRecord>)> {
        let (sender, receiver) =
            kanal::unbounded_async::<(DnsRequest, mpsc::Sender<ResolvedRecord>)>();
        tokio::spawn(async move {
            let Ok((_req, rsp_sender)) = receiver.recv().await else {
                return;
            };
            tokio::time::sleep(delay).await;
            let _ = rsp_sender.send(record).await;
        });
        sender
    }

    #[tokio::test]
    async fn later_success_overrides_earlier_error() {
        let domain = domain();
        let mut resolver =
            HickoryResolver::new(Duration::from_secs(2), Duration::from_millis(5), 1);
        resolver.push_client(spawn_server(
            Duration::from_millis(40),
            ResolvedRecord::failed(domain.clone(), 1, ResolveError::DriverTimeout),
        ));
        resolver.push_client(spawn_server(
            Duration::from_millis(120),
            ResolvedRecord::resolved(
                domain.clone(),
                60,
                1,
                300,
                vec![IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))],
            ),
        ));

        let record = resolver
            .run(domain.clone(), DnsRequest::query_ipv4(domain))
            .await;
        assert_eq!(
            record.result.unwrap(),
            vec![IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))]
        );
    }

    #[tokio::test]
    async fn later_error_is_kept_until_every_server_answers() {
        let domain = domain();
        let mut resolver =
            HickoryResolver::new(Duration::from_secs(2), Duration::from_millis(5), 1);
        resolver.push_client(spawn_server(
            Duration::from_millis(40),
            ResolvedRecord::failed(domain.clone(), 1, ResolveError::DriverTimeout),
        ));
        resolver.push_client(spawn_server(
            Duration::from_millis(120),
            ResolvedRecord::failed(domain.clone(), 1, ResolveError::NoResolverRunning),
        ));

        let record = resolver
            .run(domain.clone(), DnsRequest::query_ipv4(domain))
            .await;
        assert!(matches!(
            record.result,
            Err(ResolveError::NoResolverRunning)
        ));
    }

    #[tokio::test]
    async fn first_success_does_not_wait_for_another_server() {
        let domain = domain();
        let mut resolver =
            HickoryResolver::new(Duration::from_secs(2), Duration::from_millis(5), 1);
        resolver.push_client(spawn_server(
            Duration::from_millis(40),
            ResolvedRecord::resolved(
                domain.clone(),
                60,
                1,
                300,
                vec![IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))],
            ),
        ));
        resolver.push_client(spawn_server(
            Duration::from_millis(1500),
            ResolvedRecord::failed(domain.clone(), 1, ResolveError::DriverTimeout),
        ));

        let started = Instant::now();
        let record = resolver
            .run(domain.clone(), DnsRequest::query_ipv4(domain))
            .await;
        assert!(record.is_ok());
        assert!(started.elapsed() < Duration::from_millis(800));
    }
}
