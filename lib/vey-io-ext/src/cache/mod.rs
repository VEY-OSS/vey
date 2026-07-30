/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::hash::Hash;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

mod runtime;
pub use runtime::EffectiveCacheRuntime;

mod handle;
pub use handle::{EffectiveCacheHandle, EffectiveQueryHandle};

pub struct EffectiveCacheData<R> {
    value: Option<R>,
    expire_at: Instant,
    vanish_at: Instant,
}

impl<R> EffectiveCacheData<R> {
    pub fn inner(&self) -> Option<&R> {
        self.value.as_ref()
    }

    pub fn new(data: R, ttl: u32, vanish_wait: Duration) -> Self {
        let now = Instant::now();
        let expire_at = now
            .checked_add(Duration::from_secs(ttl as u64))
            .unwrap_or(now);
        let vanish_at = expire_at.checked_add(vanish_wait).unwrap_or(expire_at);

        EffectiveCacheData {
            value: Some(data),
            expire_at,
            vanish_at,
        }
    }

    pub fn empty(protective_ttl: u32, vanish_wait: Duration) -> Self {
        let now = Instant::now();
        let expire_at = now
            .checked_add(Duration::from_secs(protective_ttl as u64))
            .unwrap_or(now);
        let vanish_at = expire_at.checked_add(vanish_wait).unwrap_or(expire_at);
        EffectiveCacheData {
            value: None,
            expire_at,
            vanish_at,
        }
    }
}

pub struct CacheQueryRequest<K, R> {
    cache_key: Arc<K>,
    query_cache_only: bool,
    notifier: oneshot::Sender<Arc<EffectiveCacheData<R>>>,
}

pub fn create_effective_cache<K: Hash + Eq, R: Send + Sync>(
    request_batch_handle_count: NonZeroUsize,
) -> (
    EffectiveCacheRuntime<K, R>,
    EffectiveCacheHandle<K, R>,
    EffectiveQueryHandle<K, R>,
) {
    let (rsp_sender, rsp_receiver) = mpsc::unbounded_channel();
    let (query_sender, query_receiver) = mpsc::unbounded_channel();
    let (req_sender, req_receiver) = mpsc::unbounded_channel();
    let cache_runtime = EffectiveCacheRuntime::new(
        request_batch_handle_count,
        req_receiver,
        rsp_receiver,
        query_sender,
    );
    let cache_handle = EffectiveCacheHandle::new(req_sender);
    let query_handle = EffectiveQueryHandle::new(query_receiver, rsp_sender);
    (cache_runtime, cache_handle, query_handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_data_new_and_empty() {
        let data = EffectiveCacheData::new("ok".to_string(), 30, Duration::from_secs(5));
        assert_eq!(data.inner().map(String::as_str), Some("ok"));
        assert!(data.expire_at <= data.vanish_at);

        let empty = EffectiveCacheData::<String>::empty(10, Duration::from_secs(1));
        assert!(empty.inner().is_none());
        assert!(empty.expire_at <= empty.vanish_at);
    }

    #[tokio::test]
    async fn query_handle_dedups_in_flight_keys() {
        let (_runtime, _cache, mut query) =
            create_effective_cache::<String, String>(NonZeroUsize::MIN);
        let key = Arc::new("k".to_string());
        assert!(query.should_send_raw_query(key.clone(), Duration::from_secs(1)));
        assert!(!query.should_send_raw_query(key.clone(), Duration::from_secs(1)));

        query.send_rsp_data(
            key.clone(),
            EffectiveCacheData::new("v".to_string(), 1, Duration::from_secs(1)),
            false,
        );
        // After response, a new query for the same key may be sent again.
        assert!(query.should_send_raw_query(key, Duration::from_secs(1)));
    }

    #[tokio::test]
    async fn fetch_cache_only_miss_returns_none() {
        let (runtime, handle, query) = create_effective_cache::<String, String>(NonZeroUsize::MIN);
        let runtime_task = tokio::spawn(runtime);
        let miss = handle
            .fetch_cache_only(Arc::new("missing".into()), Duration::from_millis(200))
            .await;
        assert!(miss.is_none());
        drop(handle);
        drop(query);
        let _ = runtime_task.await;
    }
}
