/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::hash_map;
use std::io;
use std::net::IpAddr;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use ip_network_table::IpNetworkTable;
use log::warn;
use rustc_hash::FxHashMap;
use tokio::sync::mpsc;
use tokio::time::Instant;

use vey_geoip_types::IpLocation;

use super::{CacheQueryRequest, IpLocateServiceConfig, IpLocationCacheResponse};

struct CacheValue {
    valid_before: Instant,
    location: Arc<IpLocation>,
}

pub(crate) struct IpLocationCacheRuntime {
    request_batch_handle_count: NonZeroUsize,
    cache: IpNetworkTable<CacheValue>,
    doing: FxHashMap<IpAddr, Vec<CacheQueryRequest>>,
    req_receiver: mpsc::UnboundedReceiver<CacheQueryRequest>,
    rsp_receiver: mpsc::UnboundedReceiver<(Option<IpAddr>, IpLocationCacheResponse)>,
    query_sender: mpsc::UnboundedSender<IpAddr>,
}

impl IpLocationCacheRuntime {
    pub(crate) fn new(
        config: &IpLocateServiceConfig,
        req_receiver: mpsc::UnboundedReceiver<CacheQueryRequest>,
        rsp_receiver: mpsc::UnboundedReceiver<(Option<IpAddr>, IpLocationCacheResponse)>,
        query_sender: mpsc::UnboundedSender<IpAddr>,
    ) -> Self {
        IpLocationCacheRuntime {
            request_batch_handle_count: config.cache_request_batch_count,
            cache: IpNetworkTable::new(),
            doing: FxHashMap::default(),
            req_receiver,
            rsp_receiver,
            query_sender,
        }
    }

    fn handle_rsp(&mut self, ip: Option<IpAddr>, mut rsp: IpLocationCacheResponse) {
        if let Some(location) = rsp.value.take() {
            let net = location.network_addr();
            if let Some(queried) = ip.filter(|queried| !net.contains(*queried)) {
                warn!(
                    "discarding location for {queried}: response network {net} does not contain it"
                );
                // Drop waiters so this IP is not pinned, and do not cache the network.
                self.doing.remove(&queried);
                return;
            }

            let location = Arc::new(location);

            if let Some(ip) = ip
                && let Some(vec) = self.doing.remove(&ip)
            {
                for req in vec.into_iter() {
                    let _ = req.notifier.send(location.clone());
                }
            }

            // The returned network is this IP's smallest network. Drop each
            // longer cached prefix that still contains the IP.
            if let Some(queried) = ip {
                while let Some(cached) = self
                    .cache
                    .longest_match(queried)
                    .map(|(network, _)| network)
                    .filter(|cached| cached.netmask() > net.netmask())
                {
                    self.cache.remove(cached);
                }
            }
            self.cache.insert(
                net,
                CacheValue {
                    valid_before: rsp.expire_at,
                    location,
                },
            );
        } else if let Some(ip) = ip
            && let Some(vec) = self.doing.remove(&ip)
            && let Some((_net, v)) = self.cache.longest_match(ip)
        {
            // Always clear doing so a miss (no geo data) cannot pin the IP forever.
            // Prefer a stale cache entry when present; otherwise drop notifiers so
            // waiters observe Err and map to None.
            for req in vec.into_iter() {
                let _ = req.notifier.send(v.location.clone());
            }
        }
    }

    fn send_req(&mut self, ip: IpAddr) {
        if self.query_sender.send(ip).is_err() {
            // the query runtime should not close before the cache runtime
            unreachable!()
        }
    }

    fn handle_req(&mut self, req: CacheQueryRequest) {
        if let Some((_net, v)) = self.cache.longest_match(req.ip)
            && v.valid_before >= Instant::now()
        {
            let _ = req.notifier.send(v.location.clone());
            return;
        }

        match self.doing.entry(req.ip) {
            hash_map::Entry::Occupied(mut o) => {
                o.get_mut().push(req);
            }
            hash_map::Entry::Vacant(v) => {
                let ip = req.ip;
                v.insert(vec![req]);
                self.send_req(ip);
            }
        }
    }

    fn poll_loop(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        loop {
            // handle rsp
            loop {
                match self.rsp_receiver.poll_recv(cx) {
                    Poll::Pending => break,
                    Poll::Ready(None) => {
                        return Poll::Ready(Err(io::Error::other("query runtime quit")));
                    }
                    Poll::Ready(Some((ip, rsp))) => self.handle_rsp(ip, rsp),
                }
            }

            // handle req
            for _ in 0..self.request_batch_handle_count.get() {
                match self.req_receiver.poll_recv(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(None) => return Poll::Ready(Ok(())),
                    Poll::Ready(Some(req)) => self.handle_req(req),
                }
            }
        }
    }
}

impl Future for IpLocationCacheRuntime {
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        (*self).poll_loop(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::time::Duration;

    use tokio::sync::oneshot;

    use vey_geoip_types::IsoCountryCode;

    use crate::IpLocationCacheResponse;

    #[tokio::test]
    async fn empty_response_without_cache_clears_doing_and_allows_requery() {
        let config = IpLocateServiceConfig::default();
        let (rsp_sender, rsp_receiver) = mpsc::unbounded_channel();
        let (query_sender, mut query_receiver) = mpsc::unbounded_channel();
        let (req_sender, req_receiver) = mpsc::unbounded_channel();
        let runtime =
            IpLocationCacheRuntime::new(&config, req_receiver, rsp_receiver, query_sender);
        let runtime_task = tokio::spawn(runtime);

        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        let (notifier, waiter) = oneshot::channel();
        req_sender.send(CacheQueryRequest { ip, notifier }).unwrap();

        let queried = tokio::time::timeout(Duration::from_secs(1), query_receiver.recv())
            .await
            .expect("timed out waiting for first query")
            .expect("query channel closed");
        assert_eq!(queried, ip);

        rsp_sender
            .send((Some(ip), IpLocationCacheResponse::empty(10)))
            .unwrap();

        assert!(
            waiter.await.is_err(),
            "empty response with no cache should drop notifiers"
        );

        let (notifier2, _waiter2) = oneshot::channel();
        req_sender
            .send(CacheQueryRequest {
                ip,
                notifier: notifier2,
            })
            .unwrap();

        let queried_again = tokio::time::timeout(Duration::from_secs(1), query_receiver.recv())
            .await
            .expect("timed out waiting for re-query after empty response")
            .expect("query channel closed");
        assert_eq!(queried_again, ip);

        drop(req_sender);
        drop(rsp_sender);
        let _ = runtime_task.await;
    }

    fn location(network: &str, country: Option<IsoCountryCode>) -> IpLocation {
        let mut builder = vey_geoip_types::IpLocationBuilder::default();
        builder.set_network(network.parse().unwrap());
        if let Some(country) = country {
            builder.set_country(country);
        }
        builder.build().unwrap()
    }

    struct Harness {
        req_sender: mpsc::UnboundedSender<CacheQueryRequest>,
        rsp_sender: mpsc::UnboundedSender<(Option<IpAddr>, IpLocationCacheResponse)>,
        query_receiver: mpsc::UnboundedReceiver<IpAddr>,
        runtime_task: tokio::task::JoinHandle<io::Result<()>>,
    }

    fn harness() -> Harness {
        let config = IpLocateServiceConfig::default();
        let (rsp_sender, rsp_receiver) = mpsc::unbounded_channel();
        let (query_sender, query_receiver) = mpsc::unbounded_channel();
        let (req_sender, req_receiver) = mpsc::unbounded_channel();
        let runtime =
            IpLocationCacheRuntime::new(&config, req_receiver, rsp_receiver, query_sender);
        Harness {
            req_sender,
            rsp_sender,
            query_receiver,
            runtime_task: tokio::spawn(runtime),
        }
    }

    async fn next_query(query_receiver: &mut mpsc::UnboundedReceiver<IpAddr>) -> IpAddr {
        tokio::time::timeout(Duration::from_secs(1), query_receiver.recv())
            .await
            .expect("timed out waiting for query")
            .expect("query channel closed")
    }

    #[tokio::test]
    async fn response_outside_network_is_discarded() {
        let Harness {
            req_sender,
            rsp_sender,
            mut query_receiver,
            runtime_task,
        } = harness();
        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        let (notifier, waiter) = oneshot::channel();
        req_sender.send(CacheQueryRequest { ip, notifier }).unwrap();
        assert_eq!(next_query(&mut query_receiver).await, ip);

        rsp_sender
            .send((
                Some(ip),
                IpLocationCacheResponse::new(location("10.0.0.0/8", None), 60),
            ))
            .unwrap();
        assert!(
            waiter.await.is_err(),
            "a network that does not contain the IP must not satisfy the waiter"
        );

        let inside = IpAddr::V4(Ipv4Addr::new(10, 1, 1, 1));
        let (notifier, _waiter) = oneshot::channel();
        req_sender
            .send(CacheQueryRequest {
                ip: inside,
                notifier,
            })
            .unwrap();
        assert_eq!(next_query(&mut query_receiver).await, inside);

        let (notifier, _waiter) = oneshot::channel();
        req_sender.send(CacheQueryRequest { ip, notifier }).unwrap();
        assert_eq!(next_query(&mut query_receiver).await, ip);

        drop(req_sender);
        drop(rsp_sender);
        let _ = runtime_task.await;
    }

    #[tokio::test]
    async fn response_inside_network_is_cached() {
        let Harness {
            req_sender,
            rsp_sender,
            mut query_receiver,
            runtime_task,
        } = harness();
        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        let (notifier, waiter) = oneshot::channel();
        req_sender.send(CacheQueryRequest { ip, notifier }).unwrap();
        assert_eq!(next_query(&mut query_receiver).await, ip);

        rsp_sender
            .send((
                Some(ip),
                IpLocationCacheResponse::new(location("1.2.3.0/24", None), 60),
            ))
            .unwrap();
        let found = waiter.await.unwrap();
        assert_eq!(found.network_addr().to_string(), "1.2.3.0/24");

        let other = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 5));
        let (notifier, waiter) = oneshot::channel();
        req_sender
            .send(CacheQueryRequest {
                ip: other,
                notifier,
            })
            .unwrap();
        tokio::task::yield_now().await;
        assert!(
            query_receiver.try_recv().is_err(),
            "an address inside the cached network must not be queried again"
        );
        let found = waiter.await.unwrap();
        assert_eq!(found.network_addr().to_string(), "1.2.3.0/24");

        drop(req_sender);
        drop(rsp_sender);
        let _ = runtime_task.await;
    }

    #[tokio::test]
    async fn expired_entry_stays_until_the_same_network_is_replaced() {
        let Harness {
            req_sender,
            rsp_sender,
            mut query_receiver,
            runtime_task,
        } = harness();
        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        let (notifier, waiter) = oneshot::channel();
        req_sender.send(CacheQueryRequest { ip, notifier }).unwrap();
        assert_eq!(next_query(&mut query_receiver).await, ip);

        rsp_sender
            .send((
                Some(ip),
                IpLocationCacheResponse::new(location("1.2.3.0/24", Some(IsoCountryCode::US)), 1),
            ))
            .unwrap();
        assert_eq!(waiter.await.unwrap().country(), Some(IsoCountryCode::US));

        tokio::time::sleep(Duration::from_millis(1100)).await;

        for _ in 0..2 {
            let (notifier, waiter) = oneshot::channel();
            req_sender.send(CacheQueryRequest { ip, notifier }).unwrap();
            assert_eq!(next_query(&mut query_receiver).await, ip);
            rsp_sender
                .send((Some(ip), IpLocationCacheResponse::empty(10)))
                .unwrap();
            assert_eq!(
                waiter.await.unwrap().country(),
                Some(IsoCountryCode::US),
                "an empty response must keep the expired entry"
            );
        }

        let (notifier, waiter) = oneshot::channel();
        req_sender.send(CacheQueryRequest { ip, notifier }).unwrap();
        assert_eq!(next_query(&mut query_receiver).await, ip);
        rsp_sender
            .send((
                Some(ip),
                IpLocationCacheResponse::new(location("1.2.3.0/24", Some(IsoCountryCode::JP)), 60),
            ))
            .unwrap();
        assert_eq!(waiter.await.unwrap().country(), Some(IsoCountryCode::JP));

        let other = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 5));
        let (notifier, waiter) = oneshot::channel();
        req_sender
            .send(CacheQueryRequest {
                ip: other,
                notifier,
            })
            .unwrap();
        tokio::task::yield_now().await;
        assert!(query_receiver.try_recv().is_err());
        assert_eq!(waiter.await.unwrap().country(), Some(IsoCountryCode::JP));

        drop(req_sender);
        drop(rsp_sender);
        let _ = runtime_task.await;
    }

    #[test]
    fn returned_network_drops_more_specific_entries() {
        let config = IpLocateServiceConfig::default();
        let (_rsp_sender, rsp_receiver) = mpsc::unbounded_channel();
        let (query_sender, _query_receiver) = mpsc::unbounded_channel();
        let (_req_sender, req_receiver) = mpsc::unbounded_channel();
        let mut runtime =
            IpLocationCacheRuntime::new(&config, req_receiver, rsp_receiver, query_sender);

        let store = |runtime: &mut IpLocationCacheRuntime, ip: IpAddr, network: &str| {
            runtime.handle_rsp(
                Some(ip),
                IpLocationCacheResponse::new(location(network, None), 60),
            );
        };
        store(
            &mut runtime,
            IpAddr::V4(Ipv4Addr::new(1, 2, 9, 1)),
            "1.2.0.0/16",
        );
        store(
            &mut runtime,
            IpAddr::V4(Ipv4Addr::new(1, 2, 3, 5)),
            "1.2.3.0/25",
        );
        store(
            &mut runtime,
            IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)),
            "1.2.3.4/32",
        );
        store(
            &mut runtime,
            IpAddr::V4(Ipv4Addr::new(1, 2, 3, 200)),
            "1.2.3.200/32",
        );
        store(
            &mut runtime,
            IpAddr::V4(Ipv4Addr::new(1, 2, 4, 1)),
            "1.2.4.0/24",
        );
        store(
            &mut runtime,
            IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)),
            "1.2.3.0/24",
        );

        let matched = |runtime: &IpLocationCacheRuntime, ip: IpAddr| {
            runtime.cache.longest_match(ip).unwrap().0.to_string()
        };
        assert_eq!(
            matched(&runtime, IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))),
            "1.2.3.0/24"
        );
        assert_eq!(
            matched(&runtime, IpAddr::V4(Ipv4Addr::new(1, 2, 3, 5))),
            "1.2.3.0/24"
        );
        assert_eq!(
            matched(&runtime, IpAddr::V4(Ipv4Addr::new(1, 2, 3, 200))),
            "1.2.3.200/32"
        );
        assert_eq!(
            matched(&runtime, IpAddr::V4(Ipv4Addr::new(1, 2, 4, 1))),
            "1.2.4.0/24"
        );
        assert_eq!(
            matched(&runtime, IpAddr::V4(Ipv4Addr::new(1, 2, 9, 1))),
            "1.2.0.0/16"
        );
    }
}
