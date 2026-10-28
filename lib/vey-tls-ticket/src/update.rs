/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::future::poll_fn;
use std::sync::Arc;
use std::time::Duration;

use jiff::Timestamp;
use log::warn;
use rustc_hash::{FxHashMap, FxHashSet};
use tokio::time::Instant;
use tokio_util::time::DelayQueue;
use tokio_util::time::delay_queue::Key;

use vey_types::net::{OpensslTicketKey, RollingTicketKey, RollingTicketer, TicketKeyName};

use super::TlsTicketConfig;
use crate::source::{RemoteKeys, TicketSource};

struct ExpireEntry {
    queue_key: Key,
    remote: bool,
}

pub(crate) struct TicketKeyUpdate {
    config: TlsTicketConfig,
    ticketer: Arc<RollingTicketer<OpensslTicketKey>>,
    expire_set: FxHashMap<TicketKeyName, ExpireEntry>,
    expire_queue: DelayQueue<TicketKeyName>,
    local_roll_at: Instant,
}

impl TicketKeyUpdate {
    pub(crate) fn new(
        config: TlsTicketConfig,
        ticketer: Arc<RollingTicketer<OpensslTicketKey>>,
    ) -> Self {
        let local_roll_time = Duration::from_secs((config.local_lifetime >> 1) as u64);
        let local_roll_at = Instant::now() + local_roll_time;
        TicketKeyUpdate {
            config,
            ticketer,
            expire_set: FxHashMap::default(),
            expire_queue: DelayQueue::new(),
            local_roll_at,
        }
    }

    pub(crate) fn spawn_run(self) {
        tokio::spawn(self.run());
    }

    async fn run(mut self) {
        if self.config.check_interval.is_zero() {
            warn!("tls ticket updater stopped: check interval must be non-zero");
            return;
        }
        let mut check_interval = tokio::time::interval(self.config.check_interval);

        let remote_source = match &self.config.remote_source {
            Some(config) => match config.build() {
                Ok(source) => Some(source),
                Err(e) => {
                    warn!("remote source disabled, dur to: {e}");
                    None
                }
            },
            None => None,
        };

        loop {
            if self.expire_set.is_empty() {
                check_interval.tick().await;
                self.check_roll_ticket(remote_source.as_ref()).await;
            } else {
                tokio::select! {
                    biased;

                    _ = check_interval.tick() => {
                        self.check_roll_ticket(remote_source.as_ref()).await;
                    }
                    v = poll_fn(|cx| self.expire_queue.poll_expired(cx)) => {
                        if let Some(expired) = v {
                            self.retire_expired_key(expired.into_inner());
                        }
                    }
                }
            }

            if Arc::strong_count(&self.ticketer) == 1 {
                break;
            }
        }
    }

    async fn check_roll_ticket(&mut self, remote_source: Option<&TicketSource>) {
        let mut roll_local = true;
        if let Some(source) = &remote_source {
            match source.fetch_remote_keys().await {
                Ok(data) => {
                    roll_local = false;
                    self.apply_remote_keys(data);
                }
                Err(e) => {
                    warn!("failed to get keys from remote source: {e}")
                }
            }
        }

        let now = Instant::now();
        if roll_local && self.local_roll_at <= now {
            self.new_local_key(now);
        }
    }

    fn new_local_key(&mut self, now: Instant) {
        let local_lifetime = self.config.local_lifetime;
        match OpensslTicketKey::new_random(local_lifetime) {
            Ok(key) => self.update_encrypt_key(key, now),
            Err(e) => warn!("failed to create new ticket key: {e}"),
        }
    }

    fn apply_remote_keys(&mut self, data: RemoteKeys) {
        self.update_encrypt_key(data.enc.key, Instant::now());
        let now = Timestamp::now();
        let mut seen = FxHashSet::default();
        for dec_key in data.dec {
            let key_name = dec_key.key.name();
            if let Some(expire_dur) = dec_key.expire_duration(&now) {
                seen.insert(key_name);
                self.upsert_remote_decrypt_key(dec_key.key, key_name, expire_dur);
            } else {
                seen.remove(&key_name);
            }
        }
        self.drop_absent_remote_keys(&seen);
    }

    fn upsert_remote_decrypt_key(
        &mut self,
        key: OpensslTicketKey,
        key_name: TicketKeyName,
        expire_dur: Duration,
    ) {
        self.ticketer.add_decrypt_key(Arc::new(key));
        let existing = self.expire_set.get(&key_name).map(|entry| entry.queue_key);
        if let Some(queue_key) = existing {
            self.expire_queue.reset(&queue_key, expire_dur);
            if let Some(entry) = self.expire_set.get_mut(&key_name) {
                entry.remote = true;
            }
        } else {
            let queue_key = self.expire_queue.insert(key_name, expire_dur);
            self.expire_set.insert(
                key_name,
                ExpireEntry {
                    queue_key,
                    remote: true,
                },
            );
        }
    }

    fn drop_absent_remote_keys(&mut self, seen: &FxHashSet<TicketKeyName>) {
        let current = self.ticketer.encrypt_key().name();
        let stale = self
            .expire_set
            .iter()
            .filter(|(name, entry)| entry.remote && !seen.contains(name))
            .map(|(name, _)| *name)
            .collect::<Vec<_>>();
        for name in stale {
            if name == current {
                // The live encrypt key must stay decryptable until it rotates out.
                self.cancel_expire(name);
            } else {
                self.delete_tracked(name);
            }
        }
    }

    fn update_encrypt_key(&mut self, key: OpensslTicketKey, now: Instant) {
        let new_name = key.name();
        let old_key = self.ticketer.encrypt_key();
        let old_key_name = old_key.name();
        if old_key_name != new_name && !self.expire_set.contains_key(&old_key_name) {
            // maybe a local generated key, or a remote enc key but not in dec list
            let expire_time = Duration::from_secs(old_key.lifetime() as u64);
            let queue_key = self.expire_queue.insert(old_key_name, expire_time);
            self.expire_set.insert(
                old_key_name,
                ExpireEntry {
                    queue_key,
                    remote: false,
                },
            );
        }
        let local_roll_time = Duration::from_secs((key.lifetime() >> 1) as u64);
        self.local_roll_at = now + local_roll_time;
        let key = Arc::new(key);
        self.ticketer.set_encrypt_key(key.clone());
        self.ticketer.add_decrypt_key(key);
    }

    fn retire_expired_key(&mut self, name: TicketKeyName) {
        self.expire_set.remove(&name);
        if self.ticketer.encrypt_key().name() != name {
            self.ticketer.del_decrypt_key(name);
        }
    }

    fn cancel_expire(&mut self, name: TicketKeyName) {
        if let Some(entry) = self.expire_set.remove(&name) {
            self.expire_queue.remove(&entry.queue_key);
        }
    }

    fn delete_tracked(&mut self, name: TicketKeyName) {
        if let Some(entry) = self.expire_set.remove(&name) {
            self.expire_queue.remove(&entry.queue_key);
            self.ticketer.del_decrypt_key(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::task::{Context, Poll, Waker};

    use super::*;
    use crate::source::{RemoteDecryptKey, RemoteEncryptKey};

    fn test_config() -> TlsTicketConfig {
        TlsTicketConfig {
            check_interval: Duration::from_secs(60),
            local_lifetime: 3600,
            remote_source: None,
        }
    }

    fn ticket_key(name: u8, aes: u8, hmac: u8, lifetime: u32) -> OpensslTicketKey {
        OpensslTicketKey::new(&[name; 16], &[aes; 32], &[hmac; 16], lifetime).unwrap()
    }

    fn remote_keys(enc: OpensslTicketKey, dec: Vec<(OpensslTicketKey, i64)>) -> RemoteKeys {
        let now = Timestamp::now();
        RemoteKeys {
            enc: RemoteEncryptKey { key: enc },
            dec: dec
                .into_iter()
                .map(|(key, secs)| {
                    let expire = now
                        .checked_add(jiff::SignedDuration::from_secs(secs))
                        .unwrap();
                    RemoteDecryptKey::from_parts(key, expire)
                })
                .collect(),
        }
    }

    fn poll_expired(update: &mut TicketKeyUpdate) -> Poll<Option<TicketKeyName>> {
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        update
            .expire_queue
            .poll_expired(&mut cx)
            .map(|item| item.map(|expired| expired.into_inner()))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reinstalling_current_encrypt_key_is_not_queued() {
        let initial = ticket_key(9, 8, 7, 3600);
        let initial_name = initial.name();
        let ticketer = Arc::new(RollingTicketer::new(initial));
        let mut update = TicketKeyUpdate::new(test_config(), ticketer.clone());
        let now = Instant::now();
        let current_name = ticket_key(1, 2, 3, 0).name();

        update.update_encrypt_key(ticket_key(1, 2, 3, 0), now);
        update.update_encrypt_key(ticket_key(1, 2, 3, 0), now);

        assert!(!update.expire_set.contains_key(&current_name));
        assert!(update.expire_set.contains_key(&initial_name));
        assert!(matches!(poll_expired(&mut update), Poll::Pending));
        assert_eq!(ticketer.encrypt_key().name(), current_name);
        assert!(ticketer.get_decrypt_key(current_name.as_ref()).is_some());

        update.update_encrypt_key(ticket_key(4, 5, 6, 0), now);
        let next_name = ticket_key(4, 5, 6, 0).name();
        assert!(update.expire_set.contains_key(&current_name));
        assert!(!update.expire_set.contains_key(&next_name));
        // DelayQueue rounds a zero timeout up to the next millisecond.
        tokio::time::sleep(Duration::from_millis(20)).await;
        match poll_expired(&mut update) {
            Poll::Ready(Some(name)) => {
                assert_eq!(name, current_name);
                update.retire_expired_key(name);
            }
            _ => panic!("previous zero-lifetime key should expire immediately"),
        }
        assert!(ticketer.get_decrypt_key(current_name.as_ref()).is_none());
        assert!(ticketer.get_decrypt_key(next_name.as_ref()).is_some());
        assert_eq!(ticketer.encrypt_key().name(), next_name);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn remote_decrypt_key_tracks_material_and_expire() {
        let ticketer = Arc::new(RollingTicketer::new(ticket_key(9, 9, 9, 3600)));
        let mut update = TicketKeyUpdate::new(test_config(), ticketer.clone());
        let old = ticket_key(2, 2, 2, 100);
        let old_ticket = old.encrypt(b"ticket").unwrap();
        let name = old.name();

        update.apply_remote_keys(remote_keys(ticket_key(1, 1, 1, 7200), vec![(old, 2)]));

        let stored = ticketer.get_decrypt_key(name.as_ref()).unwrap();
        assert_eq!(stored.decrypt(&old_ticket).unwrap().unwrap(), b"ticket");
        let first_deadline = update
            .expire_queue
            .deadline(&update.expire_set.get(&name).unwrap().queue_key);

        let replacement = ticket_key(2, 4, 4, 100);
        let new_ticket = replacement.encrypt(b"ticket").unwrap();
        update.apply_remote_keys(remote_keys(
            ticket_key(1, 1, 1, 7200),
            vec![(replacement, 3600)],
        ));

        let stored = ticketer.get_decrypt_key(name.as_ref()).unwrap();
        assert!(stored.decrypt(&old_ticket).unwrap().is_none());
        assert_eq!(stored.decrypt(&new_ticket).unwrap().unwrap(), b"ticket");
        let extended_deadline = update
            .expire_queue
            .deadline(&update.expire_set.get(&name).unwrap().queue_key);
        assert!(extended_deadline > first_deadline + Duration::from_secs(60));
        assert!(matches!(poll_expired(&mut update), Poll::Pending));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn removed_remote_decrypt_key_is_dropped() {
        let ticketer = Arc::new(RollingTicketer::new(ticket_key(9, 9, 9, 3600)));
        let mut update = TicketKeyUpdate::new(test_config(), ticketer.clone());
        update.apply_remote_keys(remote_keys(
            ticket_key(1, 1, 1, 7200),
            vec![
                (ticket_key(2, 2, 2, 100), 600),
                (ticket_key(3, 3, 3, 100), 600),
            ],
        ));

        let gone = ticket_key(2, 2, 2, 100).name();
        let kept = ticket_key(3, 3, 3, 100).name();
        update.apply_remote_keys(remote_keys(
            ticket_key(1, 1, 1, 7200),
            vec![(ticket_key(3, 3, 3, 100), 600)],
        ));

        assert!(ticketer.get_decrypt_key(gone.as_ref()).is_none());
        assert!(ticketer.get_decrypt_key(kept.as_ref()).is_some());
        assert!(!update.expire_set.contains_key(&gone));
        assert!(update.expire_set.contains_key(&kept));
        assert!(matches!(poll_expired(&mut update), Poll::Pending));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn local_grace_key_survives_remote_snapshot() {
        let initial = ticket_key(9, 9, 9, 3600);
        let initial_name = initial.name();
        let ticketer = Arc::new(RollingTicketer::new(initial));
        let mut update = TicketKeyUpdate::new(test_config(), ticketer.clone());

        update.apply_remote_keys(remote_keys(ticket_key(1, 1, 1, 7200), vec![]));

        assert!(ticketer.get_decrypt_key(initial_name.as_ref()).is_some());
        assert!(!update.expire_set.get(&initial_name).unwrap().remote);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn current_encrypt_key_removed_from_remote_set_stays() {
        let ticketer = Arc::new(RollingTicketer::new(ticket_key(9, 9, 9, 3600)));
        let mut update = TicketKeyUpdate::new(test_config(), ticketer.clone());
        let enc = ticket_key(1, 1, 1, 7200);
        let enc_name = enc.name();
        let ticket = enc.encrypt(b"live").unwrap();

        update.apply_remote_keys(remote_keys(enc, vec![(ticket_key(1, 1, 1, 7200), 30)]));
        update.apply_remote_keys(remote_keys(ticket_key(1, 1, 1, 7200), vec![]));

        assert_eq!(ticketer.encrypt_key().name(), enc_name);
        assert!(!update.expire_set.contains_key(&enc_name));
        assert_eq!(
            ticketer
                .get_decrypt_key(enc_name.as_ref())
                .unwrap()
                .decrypt(&ticket)
                .unwrap()
                .unwrap(),
            b"live"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn zero_check_interval_run_does_not_panic() {
        let config = TlsTicketConfig {
            check_interval: Duration::ZERO,
            ..TlsTicketConfig::default()
        };
        let ticketer = Arc::new(RollingTicketer::new(
            OpensslTicketKey::new_random(config.local_lifetime).unwrap(),
        ));
        let _held = Arc::clone(&ticketer);
        let update = TicketKeyUpdate::new(config, ticketer);
        tokio::time::timeout(Duration::from_secs(1), update.run())
            .await
            .expect("zero check interval must not panic");
    }
}
