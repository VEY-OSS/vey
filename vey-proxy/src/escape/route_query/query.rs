/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::VecDeque;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use ahash::AHashMap;
use anyhow::anyhow;
use tokio::io::ReadBuf;
use tokio::net::UdpSocket;
use uuid::Uuid;

use vey_io_ext::{EffectiveCacheData, EffectiveQueryHandle};
use vey_types::collection::{SelectiveVec, SelectiveVecBuilder, WeightedValue};
use vey_types::metrics::NodeName;

use super::RouteQueryEscaperConfig;
use super::cache::CacheQueryKey;

pub(super) struct QueryRuntime {
    config: Arc<RouteQueryEscaperConfig>,
    socket: UdpSocket,
    query_handle: EffectiveQueryHandle<CacheQueryKey, SelectiveVec<WeightedValue<NodeName>>>,
    id_key_map: AHashMap<Uuid, Arc<CacheQueryKey>>,
    key_id_map: AHashMap<Arc<CacheQueryKey>, Uuid>,
    read_buffer: Box<[u8]>,
    write_queue: VecDeque<(Arc<CacheQueryKey>, Vec<u8>)>,
}

struct QueryResponse {
    id: Option<Uuid>,
    nodes: Vec<WeightedValue<NodeName>>,
    ttl: u32,
    error: Option<anyhow::Error>,
}

impl QueryResponse {
    fn parse(map: Vec<(rmpv::ValueRef, rmpv::ValueRef)>) -> Self {
        use anyhow::Context;
        use rmpv::ValueRef;

        let mut id = None;
        let mut nodes = Vec::new();
        let mut ttl = 0;
        let mut error = None;
        for (k, v) in map {
            let key = match vey_msgpack::value::as_string(&k) {
                Ok(key) => key,
                Err(e) => {
                    if error.is_none() {
                        error = Some(e);
                    }
                    continue;
                }
            };
            let normalized = vey_msgpack::key::normalize(key.as_str());
            // Keep reading `id` after another field fails, so that query can still be finished.
            if error.is_some() && normalized != "id" {
                continue;
            }
            match normalized.as_str() {
                "id" => match vey_msgpack::value::as_uuid(&v) {
                    Ok(parsed) => id = Some(parsed),
                    Err(e) if error.is_none() => error = Some(e),
                    Err(_) => {}
                },
                "nodes" => match v {
                    ValueRef::String(_) => {
                        match vey_msgpack::value::as_weighted_metrics_name(&v)
                            .context(format!("invalid weighted name string value for key {key}"))
                        {
                            Ok(item) => nodes.push(item),
                            Err(e) => error = Some(e),
                        }
                    }
                    ValueRef::Array(seq) => {
                        for (i, v) in seq.iter().enumerate() {
                            match vey_msgpack::value::as_weighted_metrics_name(v).context(format!(
                                "invalid weighted name string value for {key}#{i}"
                            )) {
                                Ok(item) => nodes.push(item),
                                Err(e) => {
                                    error = Some(e);
                                    break;
                                }
                            }
                        }
                    }
                    _ => error = Some(anyhow!("invalid value type for key {key}")),
                },
                "ttl" => match vey_msgpack::value::as_u32(&v)
                    .context(format!("invalid u32 value for key {key}"))
                {
                    Ok(parsed) => ttl = parsed,
                    Err(e) => error = Some(e),
                },
                _ => error = Some(anyhow!("invalid key {key}")),
            }
        }
        if error.is_none() {
            if id.is_none() {
                error = Some(anyhow!("no required key 'id' found"));
            } else {
                nodes.reverse(); // reverse as we push to the back
            }
        }
        QueryResponse {
            id,
            nodes,
            ttl,
            error,
        }
    }
}

impl QueryRuntime {
    pub(super) fn new(
        config: &Arc<RouteQueryEscaperConfig>,
        socket: UdpSocket,
        query_handle: EffectiveQueryHandle<CacheQueryKey, SelectiveVec<WeightedValue<NodeName>>>,
    ) -> Self {
        QueryRuntime {
            config: Arc::clone(config),
            socket,
            query_handle,
            id_key_map: AHashMap::new(),
            key_id_map: AHashMap::new(),
            // SAFETY: only `read_buffer[..len]` is parsed after poll_recv fills it.
            read_buffer: unsafe { Box::<[u8]>::new_uninit_slice(4096).assume_init() },
            write_queue: VecDeque::new(),
        }
    }

    fn send_empty_result(&mut self, req: Arc<CacheQueryKey>, expired: bool) {
        let result = EffectiveCacheData::empty(
            self.config.protective_cache_ttl,
            self.config.cache_vanish_wait,
        );
        self.query_handle.send_rsp_data(req, result, expired);
    }

    fn handle_req(&mut self, req: Arc<CacheQueryKey>) {
        use rmpv::ValueRef;

        if self
            .query_handle
            .should_send_raw_query(Arc::clone(&req), self.config.query_wait_timeout)
        {
            let query_id = Uuid::new_v4();

            let mut map = vec![
                (
                    ValueRef::String("id".into()),
                    ValueRef::Binary(query_id.as_bytes()),
                ),
                (
                    ValueRef::String("user".into()),
                    ValueRef::String(req.user.as_str().into()),
                ),
                (
                    ValueRef::String("host".into()),
                    ValueRef::String(req.host.as_str().into()),
                ),
            ];
            if let Some(client_ip) = &req.client_ip {
                map.push((
                    ValueRef::String("client_ip".into()),
                    ValueRef::String(client_ip.as_str().into()),
                ));
            }
            let mut buf = Vec::new();
            let v = ValueRef::Map(map);
            if rmpv::encode::write_value_ref(&mut buf, &v).is_err() {
                self.send_empty_result(req, false);
            } else {
                self.id_key_map.insert(query_id, Arc::clone(&req));
                self.key_id_map.insert(Arc::clone(&req), query_id);
                self.write_queue.push_back((req, buf));
            }
        }
    }

    fn handle_rsp(&mut self, len: usize) {
        use rmpv::ValueRef;

        let mut buf = &self.read_buffer[..len];
        let Ok(ValueRef::Map(map)) = rmpv::decode::read_value_ref(&mut buf) else {
            return;
        };
        let rsp = QueryResponse::parse(map);
        let Some(id) = rsp.id else {
            return;
        };
        let Some(req) = self.id_key_map.get(&id).cloned() else {
            return;
        };
        if rsp.error.is_some() {
            self.handle_query_failed(req, false);
            return;
        }

        let mut ttl = rsp.ttl;
        if ttl == 0 {
            ttl = self.config.protective_cache_ttl;
        } else if ttl > self.config.maximum_cache_ttl {
            ttl = self.config.maximum_cache_ttl;
        }

        let mut builder = SelectiveVecBuilder::new();
        for node in rsp.nodes {
            builder.insert(node);
        }
        let result = builder
            .build()
            .map(|nodes| EffectiveCacheData::new(nodes, ttl, self.config.cache_vanish_wait))
            .unwrap_or_else(|| EffectiveCacheData::empty(ttl, self.config.cache_vanish_wait));

        self.id_key_map.remove(&id);
        self.key_id_map.remove(&req);
        self.query_handle.send_rsp_data(req, result, false);
    }

    fn handle_query_failed(&mut self, req: Arc<CacheQueryKey>, expired: bool) {
        if let Some(id) = self.key_id_map.remove(&req) {
            self.id_key_map.remove(&id);
        }
        self.send_empty_result(req, expired);
    }

    fn poll_loop(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        loop {
            // handle rsp
            let mut buf = ReadBuf::new(&mut self.read_buffer);
            match self.socket.poll_recv(cx, &mut buf) {
                Poll::Pending => {}
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)), // TODO handle error
                Poll::Ready(Ok(_)) => {
                    let len = buf.filled().len();
                    if len > 0 {
                        self.handle_rsp(len);
                    }
                    continue;
                }
            }

            // send req from write queue
            while let Some((req, v)) = self.write_queue.pop_front() {
                match self.socket.poll_send(cx, v.as_slice()) {
                    Poll::Pending => {
                        self.write_queue.push_front((req, v));
                        break;
                    }
                    Poll::Ready(Ok(_)) => {}
                    Poll::Ready(Err(_)) => self.handle_query_failed(req, false),
                }
            }

            // handle timeout
            loop {
                match self.query_handle.poll_query_expired(cx) {
                    Poll::Pending => break,
                    Poll::Ready(None) => break,
                    Poll::Ready(Some(req)) => self.handle_query_failed(req, true),
                }
            }

            // handle req
            match self.query_handle.poll_recv_req(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Ready(Some(req)) => self.handle_req(req),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use arcstr::ArcStr;
    use rmpv::ValueRef;

    use super::*;
    use crate::config::escaper::route_query::RouteQueryEscaperConfig;

    fn runtime() -> QueryRuntime {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let socket = UdpSocket::from_std(socket).unwrap();
        let (_cache_runtime, _cache_handle, query_handle) =
            vey_io_ext::create_effective_cache(std::num::NonZeroUsize::new(1).unwrap());
        QueryRuntime::new(
            &Arc::new(RouteQueryEscaperConfig::for_test()),
            socket,
            query_handle,
        )
    }

    fn register(rt: &mut QueryRuntime) -> (Arc<CacheQueryKey>, Uuid) {
        let req = Arc::new(CacheQueryKey {
            user: ArcStr::from("user"),
            host: "example.com".to_string(),
            client_ip: None,
        });
        assert!(
            rt.query_handle
                .should_send_raw_query(Arc::clone(&req), Duration::from_secs(10))
        );
        let id = Uuid::new_v4();
        rt.id_key_map.insert(id, Arc::clone(&req));
        rt.key_id_map.insert(Arc::clone(&req), id);
        (req, id)
    }

    fn write_rsp(rt: &mut QueryRuntime, value: ValueRef<'_>) -> usize {
        let mut buf = Vec::new();
        rmpv::encode::write_value_ref(&mut buf, &value).unwrap();
        rt.read_buffer[..buf.len()].copy_from_slice(&buf);
        buf.len()
    }

    #[tokio::test]
    async fn a_response_with_an_unknown_key_fails_that_query_immediately() {
        let mut rt = runtime();
        let (req, id) = register(&mut rt);
        let id_bytes = *id.as_bytes();
        let len = write_rsp(
            &mut rt,
            ValueRef::Map(vec![
                (ValueRef::String("id".into()), ValueRef::Binary(&id_bytes)),
                (
                    ValueRef::String("extra".into()),
                    ValueRef::String("nope".into()),
                ),
            ]),
        );
        rt.handle_rsp(len);
        assert!(rt.id_key_map.is_empty());
        assert!(rt.key_id_map.is_empty());
        assert!(
            rt.query_handle
                .should_send_raw_query(req, Duration::from_secs(10))
        );
    }

    #[tokio::test]
    async fn a_datagram_without_an_id_leaves_the_query_waiting() {
        let mut rt = runtime();
        let (_req, id) = register(&mut rt);
        let len = write_rsp(&mut rt, ValueRef::String("not a map".into()));
        rt.handle_rsp(len);
        assert!(rt.id_key_map.contains_key(&id));
        assert_eq!(rt.key_id_map.len(), 1);
    }
}

impl Future for QueryRuntime {
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        (*self).poll_loop(cx)
    }
}
