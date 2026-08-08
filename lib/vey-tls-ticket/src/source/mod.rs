/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use anyhow::Context;
use jiff::Timestamp;

use std::time::Duration;

use vey_types::net::OpensslTicketKey;

mod json;
#[cfg(feature = "yaml")]
mod yaml;

mod redis;
use redis::{RedisSource, RedisSourceConfig};

const CONFIG_KEY_SOURCE_TYPE: &str = "type";

pub(crate) struct RemoteEncryptKey {
    pub(crate) key: OpensslTicketKey,
}

pub(crate) struct RemoteDecryptKey {
    pub(crate) key: OpensslTicketKey,
    expire: Timestamp,
}

impl RemoteDecryptKey {
    pub(crate) fn expire_duration(&self, now: &Timestamp) -> Option<Duration> {
        Duration::try_from(self.expire.duration_since(*now)).ok()
    }
}

pub(crate) struct RemoteKeys {
    pub(crate) enc: RemoteEncryptKey,
    pub(crate) dec: Vec<RemoteDecryptKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TicketSourceConfig {
    Redis(RedisSourceConfig),
}

impl TicketSourceConfig {
    pub(crate) fn build(&self) -> anyhow::Result<TicketSource> {
        match self {
            TicketSourceConfig::Redis(s) => {
                let source = s
                    .build()
                    .context("failed to build redis remote key source")?;
                Ok(TicketSource::Redis(source))
            }
        }
    }
}

pub(crate) enum TicketSource {
    Redis(RedisSource),
}

impl TicketSource {
    pub(crate) async fn fetch_remote_keys(&self) -> anyhow::Result<RemoteKeys> {
        match self {
            TicketSource::Redis(s) => s
                .fetch_remote_keys()
                .await
                .context("failed to fetch remote keys from redis"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vey_types::net::OpensslTicketKeyBuilder;

    // Helper function to create a test RemoteDecryptKey
    fn create_test_decrypt_key(expire: Timestamp) -> RemoteDecryptKey {
        let builder = OpensslTicketKeyBuilder::default();
        RemoteDecryptKey {
            key: builder.build(),
            expire,
        }
    }

    #[test]
    fn remote_decrypt_key_expire_duration_future() {
        let future_time: Timestamp = "2025-12-31T23:59:59Z".parse().unwrap();
        let now: Timestamp = "2025-06-15T12:00:00Z".parse().unwrap();

        let key = create_test_decrypt_key(future_time);
        let duration = key.expire_duration(&now);

        assert!(duration.is_some());
        let dur = duration.unwrap();
        assert_eq!(
            dur.as_secs(),
            Duration::try_from(future_time.duration_since(now))
                .unwrap()
                .as_secs()
        );
    }

    #[test]
    fn remote_decrypt_key_expire_duration_past() {
        let past_time: Timestamp = "2024-01-01T00:00:00Z".parse().unwrap();
        let now: Timestamp = "2025-06-15T12:00:00Z".parse().unwrap();

        let key = create_test_decrypt_key(past_time);
        let duration = key.expire_duration(&now);

        assert!(duration.is_none());
    }
}
