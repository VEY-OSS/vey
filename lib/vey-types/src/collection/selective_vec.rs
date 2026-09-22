/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::cmp::Ordering;
use std::hash::{BuildHasher, Hash, Hasher};
use std::str::FromStr;
use std::sync::atomic;

use foldhash::fast::FixedState;
use rand::seq::IndexedRandom;
use smallvec::SmallVec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectivePickPolicy {
    Random,
    Serial,
    RoundRobin,
    Ketama,
    Rendezvous,
    JumpHash,
}

impl FromStr for SelectivePickPolicy {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "random" => Ok(SelectivePickPolicy::Random),
            "serial" | "sequence" => Ok(SelectivePickPolicy::Serial),
            "roundrobin" | "rr" | "round_robin" => Ok(SelectivePickPolicy::RoundRobin),
            "ketama" => Ok(SelectivePickPolicy::Ketama),
            "rendezvous" => Ok(SelectivePickPolicy::Rendezvous),
            "jump" | "jumphash" | "jump_hash" => Ok(SelectivePickPolicy::JumpHash),
            _ => Err(()),
        }
    }
}

pub trait SelectiveItem {
    fn weight(&self) -> f64;
    fn weight_u32(&self) -> u32 {
        // return the smallest integer greater than or equal to `self`
        let w = self.weight().ceil();
        if !w.is_finite() || w <= 0.0 {
            0
        } else {
            w.min(f64::from(u32::MAX)) as u32
        }
    }
    fn selective_hash<H: Hasher>(&self, state: &mut H);
}

impl<T: Hash> SelectiveItem for T {
    fn weight(&self) -> f64 {
        1.0
    }

    fn weight_u32(&self) -> u32 {
        1
    }

    fn selective_hash<H: Hasher>(&self, state: &mut H) {
        self.hash(state);
    }
}

pub struct SelectiveVecBuilder<T> {
    inner: Vec<T>,
}

impl<T: SelectiveItem> SelectiveVecBuilder<T> {
    pub fn new() -> Self {
        SelectiveVecBuilder { inner: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        SelectiveVecBuilder {
            inner: Vec::with_capacity(capacity),
        }
    }

    pub fn with_inner(inner: Vec<T>) -> Self {
        SelectiveVecBuilder { inner }
    }

    pub fn insert(&mut self, value: T) {
        self.inner.push(value);
    }

    pub fn build(self) -> Option<SelectiveVec<T>> {
        if self.inner.is_empty() {
            return None;
        }

        let mut weighted = false;
        let weight = self.inner[0].weight();
        for item in &self.inner {
            if item.weight().ne(&weight) {
                weighted = true;
                break;
            }
        }

        let mut nodes = self.inner;
        // reserve order for equal nodes
        nodes.sort_by(|a, b| {
            b.weight()
                .partial_cmp(&a.weight())
                .unwrap_or(Ordering::Equal)
        });

        let ketama_ring = ketama_ring_create(&nodes);
        let rr_seq = if weighted {
            swrr_seq(&nodes)
        } else {
            Vec::new()
        };

        Some(SelectiveVec {
            weighted,
            inner: nodes,
            rr_id: atomic::AtomicUsize::new(0),
            rr_seq,
            ketama_ring,
        })
    }
}

fn gcd_u64(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

/// One nginx smooth weighted round-robin step over fixed integer weights.
fn swrr_step(current: &mut [i64], weight: &[i64], total: i64) -> usize {
    let mut best = 0;
    let mut best_weight = i64::MIN;
    for (i, w) in weight.iter().copied().enumerate() {
        if w <= 0 {
            continue;
        }
        current[i] = current[i].saturating_add(w);
        if current[i] > best_weight {
            best_weight = current[i];
            best = i;
        }
    }
    current[best] = current[best].saturating_sub(total);
    best
}

/// Smooth weighted round-robin index cycle.
///
/// Weights are fixed at build time, so the sequence repeats and can be walked
/// with an atomic cursor. Weights are reduced by their gcd first. A cycle longer
/// than 65536 (or the peer count, whichever is larger) is scaled down with the
/// largest-remainder method.
fn swrr_seq<T: SelectiveItem>(nodes: &[T]) -> Vec<usize> {
    let mut weight: Vec<u64> = nodes
        .iter()
        .map(|node| u64::from(node.weight_u32()))
        .collect();
    let mut g = 0u64;
    for w in weight.iter().copied() {
        if w == 0 {
            continue;
        }
        g = if g == 0 { w } else { gcd_u64(g, w) };
    }
    if g == 0 {
        return Vec::new();
    }
    if g > 1 {
        for w in &mut weight {
            *w /= g;
        }
    }
    fit_rr_cycle(&mut weight);

    let total: u64 = weight.iter().copied().sum();
    let Ok(total_i) = i64::try_from(total) else {
        return Vec::new();
    };
    if total_i == 0 {
        return Vec::new();
    }
    let weight_i: Vec<i64> = weight.iter().copied().map(|w| w as i64).collect();
    let mut current = vec![0i64; weight_i.len()];
    let mut seq = Vec::with_capacity(total as usize);
    for _ in 0..total {
        seq.push(swrr_step(&mut current, &weight_i, total_i));
    }
    seq
}

fn fit_rr_cycle(weight: &mut [u64]) {
    const MAX_RR_CYCLE: u64 = 65536;
    let sum: u64 = weight.iter().copied().sum();
    let peers = weight.iter().filter(|w| **w > 0).count() as u64;
    let cap = MAX_RR_CYCLE.max(peers);
    if sum <= cap || sum == 0 {
        return;
    }

    let mut fitted = vec![0u64; weight.len()];
    let mut used = 0u64;
    for (i, w) in weight.iter().copied().enumerate() {
        fitted[i] = w.saturating_mul(cap) / sum;
        used = used.saturating_add(fitted[i]);
    }
    let mut rem = cap.saturating_sub(used);
    let mut order: Vec<usize> = (0..weight.len()).collect();
    order.sort_by(|&a, &b| {
        let fa = weight[a].saturating_mul(cap) % sum;
        let fb = weight[b].saturating_mul(cap) % sum;
        fb.cmp(&fa)
    });
    while rem > 0 {
        let mut progressed = false;
        for i in order.iter().copied() {
            if rem == 0 {
                break;
            }
            if weight[i] == 0 {
                continue;
            }
            fitted[i] = fitted[i].saturating_add(1);
            rem -= 1;
            progressed = true;
        }
        if !progressed {
            break;
        }
    }
    weight.copy_from_slice(&fitted);
}

fn ketama_ring_create<T: SelectiveItem>(nodes: &[T]) -> Vec<(usize, u32)> {
    // This constant is copied from nginx. It will create 160 points per weight unit. For
    // example, a weight of 2 will create 320 points on the ring.
    const POINT_MULTIPLE: u32 = 160;
    let mut total_weights: u32 = 0;
    for v in nodes {
        total_weights = total_weights.saturating_add(v.weight_u32());
    }
    let capacity = (total_weights as usize).saturating_mul(POINT_MULTIPLE as usize);
    let mut ring = Vec::with_capacity(capacity);

    for (i, node) in nodes.iter().enumerate() {
        let mut hasher = crc32fast::Hasher::new();
        node.selective_hash(&mut hasher);

        let num_points = node.weight_u32().saturating_mul(POINT_MULTIPLE);

        for j in 0..num_points {
            let mut hasher = hasher.clone();
            hasher.update(&j.to_le_bytes());

            let hash = hasher.finalize();
            ring.push((i, hash));
        }
    }

    // Sort and remove any duplicates.
    ring.sort_unstable_by_key(|v| v.1);
    ring.dedup_by(|v1, v2| v1.1 == v2.1);

    ring
}

impl<T: SelectiveItem> Default for SelectiveVecBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SelectiveVec<T: SelectiveItem> {
    weighted: bool,
    inner: Vec<T>,
    rr_id: atomic::AtomicUsize,
    /// Smooth weighted round-robin index cycle. Empty when peers are unweighted.
    rr_seq: Vec<usize>,
    ketama_ring: Vec<(usize, u32)>,
}

macro_rules! panic_on_empty {
    () => {{ panic!("do panic check before pick node") }};
}

impl<T: SelectiveItem> SelectiveVec<T> {
    #[cfg(feature = "resolve")]
    pub(crate) fn new_basic(inner: Vec<T>) -> Self {
        debug_assert!(!inner.is_empty());
        SelectiveVec {
            weighted: false,
            inner,
            rr_id: Default::default(),
            rr_seq: Vec::new(),
            ketama_ring: Vec::new(),
        }
    }

    pub fn pick_random(&self) -> &T {
        match self.inner.len() {
            0 => panic_on_empty!(),
            1 => &self.inner[0],
            _ => {
                let mut rng = rand::rng();
                if self.weighted {
                    self.inner
                        .choose_weighted(&mut rng, |v| v.weight())
                        .unwrap_or(&self.inner[0])
                } else {
                    self.inner.choose(&mut rng).unwrap_or(&self.inner[0])
                }
            }
        }
    }

    pub fn pick_random_n(&self, n: usize) -> Vec<&T> {
        match self.inner.len() {
            0 => panic_on_empty!(),
            1 => vec![&self.inner[0]],
            _ => {
                let len = self.inner.len().min(n);

                let mut rng = rand::rng();
                if self.weighted {
                    self.inner
                        .sample_weighted(&mut rng, len, |v| v.weight())
                        .unwrap_or_else(|_| self.inner.sample(&mut rng, len))
                        .collect()
                } else {
                    self.inner.sample(&mut rng, len).collect()
                }
            }
        }
    }

    pub fn pick_serial(&self) -> &T {
        if self.inner.is_empty() {
            panic_on_empty!()
        } else {
            &self.inner[0]
        }
    }

    pub fn pick_serial_n(&self, n: usize) -> Vec<&T> {
        if self.inner.is_empty() {
            panic_on_empty!()
        } else {
            let mut len = self.inner.len();
            if len > n {
                len = n;
            }

            let mut r = Vec::with_capacity(len);
            for item in &self.inner.as_slice()[0..len] {
                r.push(item);
            }
            r
        }
    }

    pub fn pick_round_robin(&self) -> &T {
        match self.inner.len() {
            0 => panic_on_empty!(),
            1 => &self.inner[0],
            len => {
                let cycle = if self.weighted {
                    self.rr_seq.len()
                } else {
                    len
                };
                if cycle == 0 {
                    return &self.inner[0];
                }
                let id =
                    self.rr_id
                        .update(atomic::Ordering::AcqRel, atomic::Ordering::Acquire, |id| {
                            let next = id + 1;
                            if next >= cycle { 0 } else { next }
                        });
                let idx = if self.weighted {
                    self.rr_seq.get(id).copied().unwrap_or(0)
                } else {
                    id
                };
                self.inner.get(idx).unwrap_or(&self.inner[0])
            }
        }
    }

    pub fn pick_round_robin_n(&self, n: usize) -> Vec<&T> {
        match self.inner.len() {
            0 => panic_on_empty!(),
            1 => vec![&self.inner[0]],
            len => {
                if self.weighted {
                    let n = n.min(len);
                    let seq_len = self.rr_seq.len();
                    if n == 0 {
                        return Vec::new();
                    }
                    if seq_len == 0 {
                        return vec![&self.inner[0]; n];
                    }
                    let start = self.rr_id.update(
                        atomic::Ordering::AcqRel,
                        atomic::Ordering::Acquire,
                        |id| (id + n) % seq_len,
                    );
                    let mut r = Vec::with_capacity(n);
                    for i in 0..n {
                        let seq_i = (start + i) % seq_len;
                        let idx = self.rr_seq.get(seq_i).copied().unwrap_or(0);
                        r.push(&self.inner[idx]);
                    }
                    return r;
                }
                let n = n.min(len);
                let next_end = |id: usize| {
                    let mut end = id + n;
                    if end >= len {
                        end %= len;
                    }
                    end
                };
                let id = self.rr_id.update(
                    atomic::Ordering::AcqRel,
                    atomic::Ordering::Acquire,
                    next_end,
                );
                let end = next_end(id);
                let mut r = Vec::with_capacity(n);
                if end <= id {
                    for item in &self.inner.as_slice()[id..] {
                        r.push(item);
                    }
                    for item in &self.inner.as_slice()[0..end] {
                        r.push(item);
                    }
                } else {
                    for item in &self.inner.as_slice()[id..end] {
                        r.push(item);
                    }
                }
                r
            }
        }
    }

    /// It outputs a bucket number in the range [0, slot_count)
    fn jump_hash<K>(key: &K, slot_count: u32) -> u32
    where
        K: Hash + ?Sized,
    {
        // The classic jump consistent hash uses f64→i64 in the loop. Values at or
        // above 2^30 can saturate that cast and fail to converge, so clamp.
        debug_assert!(slot_count > 0);
        let slot_count = slot_count.min((1 << 30) - 1);

        let mut h = FixedState::default().hash_one(key);
        let (mut b, mut j) = (-1i64, 0i64);
        while j < i64::from(slot_count) {
            b = j;
            h = h.wrapping_mul(2862933555777941757).wrapping_add(1);
            j = ((b.wrapping_add(1) as f64) * (((1u64 << 31) as f64) / (((h >> 33) + 1) as f64)))
                as i64;
        }
        b as u32
    }

    pub fn pick_jump<K>(&self, key: &K) -> &T
    where
        K: Hash + ?Sized,
    {
        match self.inner.len() {
            0 => panic_on_empty!(),
            1 => &self.inner[0],
            slot_count => {
                // no weight support
                let slot = Self::jump_hash(key, slot_count as u32);
                self.inner.get(slot as usize).unwrap_or(&self.inner[0])
            }
        }
    }

    fn rendezvous_hash<K>(item: &T, key: &K) -> u64
    where
        K: Hash + ?Sized,
    {
        let mut hasher = FixedState::default().build_hasher();
        key.hash(&mut hasher);
        item.selective_hash(&mut hasher);
        hasher.finish()
    }

    fn rendezvous_weighted_hash<K>(item: &T, key: &K) -> f64
    where
        K: Hash + ?Sized,
    {
        let weight = item.weight();
        if !weight.is_finite() || weight <= 0.0 {
            // Zero / non-finite weight must not produce Inf/NaN that poisons ordering.
            return f64::NEG_INFINITY;
        }

        let mut hasher = FixedState::default().build_hasher();
        key.hash(&mut hasher);
        item.selective_hash(&mut hasher);
        let hash = hasher.finish() as f64;
        let distance = (hash / u64::MAX as f64).ln();
        distance / weight
    }

    pub fn pick_rendezvous<K>(&self, key: &K) -> &T
    where
        K: Hash + ?Sized,
    {
        match self.inner.len() {
            0 => panic_on_empty!(),
            1 => &self.inner[0],
            _ => {
                let mut node = &self.inner[0];
                if self.weighted {
                    let mut final_value = 0f64;
                    for item in &self.inner {
                        let value = Self::rendezvous_weighted_hash(item, key);
                        if final_value < value {
                            final_value = value;
                            node = item;
                        }
                    }
                } else {
                    let mut final_value = 0u64;
                    for item in &self.inner {
                        let value = Self::rendezvous_hash(item, key);
                        if final_value < value {
                            final_value = value;
                            node = item;
                        }
                    }
                }
                node
            }
        }
    }

    pub fn pick_rendezvous_n<K>(&self, key: &K, n: usize) -> Vec<&T>
    where
        K: Hash + ?Sized,
    {
        match self.inner.len() {
            0 => panic_on_empty!(),
            1 => vec![&self.inner[0]],
            _ => {
                // use stack storage if less than or equal to 32 nodes
                let mut nodes = SmallVec::<[(&T, f64); 32]>::with_capacity(self.inner.len());
                if self.weighted {
                    for item in &self.inner {
                        let value = Self::rendezvous_weighted_hash(item, key);
                        nodes.push((item, value));
                    }
                } else {
                    for item in &self.inner {
                        let value = Self::rendezvous_hash(item, key) as f64;
                        nodes.push((item, value));
                    }
                }

                nodes.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
                if n < nodes.len() {
                    nodes.truncate(n);
                }
                nodes.into_iter().map(|n| n.0).collect()
            }
        }
    }

    fn ketama_ring_idx<K>(&self, key: &K) -> usize
    where
        K: Hash + ?Sized,
    {
        let mut hasher = crc32fast::Hasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finalize();

        match self.ketama_ring.binary_search_by(|v| v.1.cmp(&hash)) {
            Ok(i) => i, // found
            Err(i) => {
                // will be inserted here
                if i >= self.ketama_ring.len() {
                    // make sure we always get a valid node
                    0
                } else {
                    i
                }
            }
        }
    }

    pub fn pick_ketama<K>(&self, key: &K) -> &T
    where
        K: Hash + ?Sized,
    {
        match self.inner.len() {
            0 => panic_on_empty!(),
            1 => &self.inner[0],
            _ => {
                if self.ketama_ring.is_empty() {
                    // All nodes had non-positive weight_u32; fall back to first node.
                    return &self.inner[0];
                }
                let idx = self.ketama_ring_idx(key);
                let node = &self.ketama_ring[idx];
                &self.inner[node.0]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Node {
        name: String,
        weight: f64,
    }

    impl SelectiveItem for Node {
        fn weight(&self) -> f64 {
            self.weight
        }

        fn selective_hash<H: Hasher>(&self, state: &mut H) {
            self.name.hash(state);
        }
    }

    impl PartialEq for Node {
        fn eq(&self, other: &Self) -> bool {
            self.name.eq(&other.name)
        }
    }

    #[test]
    fn pick_one_from_one() {
        let node = Node {
            name: "test".to_string(),
            weight: 1f64,
        };

        let mut builder = SelectiveVecBuilder::with_capacity(1);
        builder.insert(node.clone());
        let vec = builder.build().unwrap();

        assert!(node.eq(vec.pick_serial()));
        assert!(node.eq(vec.pick_round_robin()));
        assert!(node.eq(vec.pick_random()));
        assert!(node.eq(vec.pick_rendezvous("k")));
        assert!(node.eq(vec.pick_jump("k")));
        assert!(node.eq(vec.pick_ketama("k")));
    }

    #[test]
    fn pick_one_from_two() {
        let node1 = Node {
            name: "node1".to_string(),
            weight: 1f64,
        };
        let node2 = Node {
            name: "node2".to_string(),
            weight: 1f64,
        };

        let mut builder = SelectiveVecBuilder::with_capacity(1);
        builder.insert(node1.clone());
        builder.insert(node2.clone());
        let vec = builder.build().unwrap();

        assert!(node1.eq(vec.pick_serial()));
        assert!(node1.eq(vec.pick_round_robin()));
        assert!(node2.eq(vec.pick_round_robin()));

        /*
        let mut see1 = false;
        let mut see2 = false;
        for _ in 0..100 {
            let node = vec.pick_random();
            if node.eq(&node1) {
                see1 = true;
            }
            if node.eq(&node2) {
                see2 = true;
            }
        }
        assert!(see1);
        assert!(see2);
         */

        let prev = vec.pick_rendezvous("k");
        let next = vec.pick_rendezvous("k");
        assert!(prev.eq(next));

        let prev = vec.pick_jump("k");
        let next = vec.pick_jump("k");
        assert!(prev.eq(next));

        let prev = vec.pick_ketama("k");
        let next = vec.pick_ketama("k");
        assert!(prev.eq(next));
    }

    #[test]
    fn pick_one_from_weighted_two() {
        let node1 = Node {
            name: "node1".to_string(),
            weight: 1f64,
        };
        let node2 = Node {
            name: "node2".to_string(),
            weight: 2f64,
        };

        let mut builder = SelectiveVecBuilder::with_capacity(1);
        builder.insert(node1.clone());
        builder.insert(node2.clone());
        let vec = builder.build().unwrap();

        assert!(node2.eq(vec.pick_serial()));
        assert!(node2.eq(vec.pick_round_robin()));
        assert!(node1.eq(vec.pick_round_robin()));
        assert!(node2.eq(vec.pick_round_robin()));

        /*
        let mut see1 = 0usize;
        let mut see2 = 0usize;
        for _ in 0..100 {
            let node = vec.pick_random();
            if node.eq(&node1) {
                see1 += 1;
            }
            if node.eq(&node2) {
                see2 += 1;
            }
        }
        assert!(see2 > see1);
         */

        let prev = vec.pick_rendezvous("k");
        let next = vec.pick_rendezvous("k");
        assert!(prev.eq(next));

        let prev = vec.pick_jump("k");
        let next = vec.pick_jump("k");
        assert!(prev.eq(next));

        let prev = vec.pick_ketama("k");
        let next = vec.pick_ketama("k");
        assert!(prev.eq(next));
    }

    #[test]
    fn pick_two_from_two() {
        let node1 = Node {
            name: "node1".to_string(),
            weight: 1f64,
        };
        let node2 = Node {
            name: "node2".to_string(),
            weight: 1f64,
        };

        let mut builder = SelectiveVecBuilder::with_capacity(1);
        builder.insert(node1.clone());
        builder.insert(node2.clone());
        let vec = builder.build().unwrap();

        let r = vec.pick_serial_n(2);
        assert_eq!(r.len(), 2);
        assert!(r[0].eq(&node1));
        assert!(r[1].eq(&node2));

        let r = vec.pick_round_robin_n(2);
        assert_eq!(r.len(), 2);
        assert!(r[0].eq(&node1));
        assert!(r[1].eq(&node2));

        /*
        let mut see1 = false;
        let mut see2 = false;
        let r = vec.pick_random_n(2);
        assert_eq!(r.len(), 2);
        assert!(r[0].ne(r[1]));
        for item in &r {
            if node1.eq(item) {
                see1 = true;
            }
            if node2.eq(item) {
                see2 = true;
            }
        }
        assert!(see1);
        assert!(see2);
         */

        let r1 = vec.pick_rendezvous_n("k", 2);
        let r2 = vec.pick_rendezvous_n("k", 2);
        assert_eq!(r1.len(), 2);
        assert_eq!(r2.len(), 2);
        assert!(r1[0].ne(r1[1]));
        assert!(r1[0].eq(r2[0]));
        assert!(r1[1].eq(r2[1]));
    }

    #[test]
    fn pick_two_from_weighted_two() {
        let node1 = Node {
            name: "node1".to_string(),
            weight: 1f64,
        };
        let node2 = Node {
            name: "node2".to_string(),
            weight: 2f64,
        };

        let mut builder = SelectiveVecBuilder::with_capacity(1);
        builder.insert(node1.clone());
        builder.insert(node2.clone());
        let vec = builder.build().unwrap();

        let r = vec.pick_serial_n(2);
        assert_eq!(r.len(), 2);
        assert!(r[0].eq(&node2));
        assert!(r[1].eq(&node1));

        let r = vec.pick_round_robin_n(2);
        assert_eq!(r.len(), 2);
        assert!(r[0].eq(&node2));
        assert!(r[1].eq(&node1));

        /*
        let mut see1 = false;
        let mut see2 = false;
        let r = vec.pick_random_n(2);
        assert_eq!(r.len(), 2);
        assert!(r[0].ne(r[1]));
        for item in &r {
            if node1.eq(item) {
                see1 = true;
            }
            if node2.eq(item) {
                see2 = true;
            }
        }
        assert!(see1);
        assert!(see2);
         */

        let r1 = vec.pick_rendezvous_n("k", 2);
        let r2 = vec.pick_rendezvous_n("k", 2);
        assert_eq!(r1.len(), 2);
        assert_eq!(r2.len(), 2);
        assert!(r1[0].ne(r1[1]));
        assert!(r1[0].eq(r2[0]));
        assert!(r1[1].eq(r2[1]));
    }

    #[test]
    fn pick_two_from_three() {
        let node1 = Node {
            name: "node1".to_string(),
            weight: 1f64,
        };
        let node2 = Node {
            name: "node2".to_string(),
            weight: 1f64,
        };
        let node3 = Node {
            name: "node3".to_string(),
            weight: 1f64,
        };

        let mut builder = SelectiveVecBuilder::with_capacity(1);
        builder.insert(node1.clone());
        builder.insert(node2.clone());
        builder.insert(node3.clone());
        let vec = builder.build().unwrap();

        let r = vec.pick_serial_n(2);
        assert_eq!(r.len(), 2);
        assert!(r[0].eq(&node1));
        assert!(r[1].eq(&node2));

        let r = vec.pick_round_robin_n(2);
        assert_eq!(r.len(), 2);
        assert!(r[0].eq(&node1));
        assert!(r[1].eq(&node2));

        let r = vec.pick_round_robin_n(2);
        assert_eq!(r.len(), 2);
        assert!(r[0].eq(&node3));
        assert!(r[1].eq(&node1));

        /*
        let mut see1 = false;
        let mut see2 = false;
        for _ in 0..100 {
            let r = vec.pick_random_n(2);
            assert_eq!(r.len(), 2);
            assert!(r[0].ne(r[1]));
            for item in &r {
                if node1.eq(item) {
                    see1 = true;
                }
                if node2.eq(item) {
                    see2 = true;
                }
            }
        }
        assert!(see1);
        assert!(see2);
         */

        let r1 = vec.pick_rendezvous_n("k", 2);
        let r2 = vec.pick_rendezvous_n("k", 2);
        assert_eq!(r1.len(), 2);
        assert_eq!(r2.len(), 2);
        assert!(r1[0].ne(r1[1]));
        assert!(r1[0].eq(r2[0]));
        assert!(r1[1].eq(r2[1]));
    }

    #[test]
    fn pick_two_from_weighted_three() {
        let node1 = Node {
            name: "node1".to_string(),
            weight: 1f64,
        };
        let node2 = Node {
            name: "node2".to_string(),
            weight: 2f64,
        };
        let node3 = Node {
            name: "node3".to_string(),
            weight: 3f64,
        };

        let mut builder = SelectiveVecBuilder::with_capacity(1);
        builder.insert(node1.clone());
        builder.insert(node2.clone());
        builder.insert(node3.clone());
        let vec = builder.build().unwrap();

        let r = vec.pick_serial_n(2);
        assert_eq!(r.len(), 2);
        assert!(r[0].eq(&node3));
        assert!(r[1].eq(&node2));

        let r = vec.pick_round_robin_n(2);
        assert_eq!(r.len(), 2);
        assert!(r[0].eq(&node3));
        assert!(r[1].eq(&node2));

        let r = vec.pick_round_robin_n(2);
        assert_eq!(r.len(), 2);
        assert!(r[0].eq(&node3));
        assert!(r[1].eq(&node1));

        /*
        let mut see1 = 0usize;
        let mut see2 = 0usize;
        let mut see3 = 0usize;
        for _ in 0..100 {
            let r = vec.pick_random_n(2);
            assert_eq!(r.len(), 2);
            assert!(r[0].ne(r[1]));
            for item in r {
                if node1.eq(item) {
                    see1 += 1;
                }
                if node2.eq(item) {
                    see2 += 1;
                }
                if node3.eq(item) {
                    see3 += 1;
                }
            }
        }
        assert!(see3 > see2);
        assert!(see2 > see1);
         */

        let r1 = vec.pick_rendezvous_n("k", 2);
        let r2 = vec.pick_rendezvous_n("k", 2);
        assert_eq!(r1.len(), 2);
        assert_eq!(r2.len(), 2);
        assert!(r1[0].ne(r1[1]));
        assert!(r1[0].eq(r2[0]));
        assert!(r1[1].eq(r2[1]));
    }

    #[test]
    fn round_robin_smooth_weighted_cycle() {
        let node1 = Node {
            name: "node1".to_string(),
            weight: 1f64,
        };
        let node2 = Node {
            name: "node2".to_string(),
            weight: 2f64,
        };
        let node3 = Node {
            name: "node3".to_string(),
            weight: 3f64,
        };
        let zero = Node {
            name: "zero".to_string(),
            weight: 0.0,
        };

        let mut builder = SelectiveVecBuilder::with_capacity(4);
        builder.insert(node1.clone());
        builder.insert(zero);
        builder.insert(node2.clone());
        builder.insert(node3.clone());
        let vec = builder.build().unwrap();

        let mut names = Vec::new();
        for _ in 0..6 {
            names.push(vec.pick_round_robin().name.clone());
        }
        assert_eq!(
            names,
            ["node3", "node2", "node3", "node1", "node2", "node3"]
        );

        let mut names = Vec::new();
        for _ in 0..6 {
            names.push(vec.pick_round_robin().name.clone());
        }
        assert_eq!(
            names,
            ["node3", "node2", "node3", "node1", "node2", "node3"]
        );
    }

    #[test]
    fn jump_hash_clamps_large_slot_count() {
        let slot = SelectiveVec::<Node>::jump_hash("key", u32::MAX);
        assert!(slot < (1 << 30));
    }

    #[test]
    fn rendezvous_zero_weight_never_wins() {
        let zero = Node {
            name: "zero".to_string(),
            weight: 0.0,
        };
        let positive = Node {
            name: "positive".to_string(),
            weight: 1.0,
        };

        let mut builder = SelectiveVecBuilder::with_capacity(2);
        builder.insert(zero);
        builder.insert(positive.clone());
        let vec = builder.build().unwrap();

        for i in 0..64u32 {
            assert!(
                positive.eq(vec.pick_rendezvous(&i)),
                "zero-weight node must not be selected for key {i}"
            );
        }
    }

    #[test]
    fn ketama_all_zero_weight_falls_back() {
        let node = Node {
            name: "zero".to_string(),
            weight: 0.0,
        };
        let mut builder = SelectiveVecBuilder::with_capacity(2);
        builder.insert(node.clone());
        builder.insert(Node {
            name: "also_zero".to_string(),
            weight: 0.0,
        });
        let vec = builder.build().unwrap();
        assert!(node.eq(vec.pick_ketama("k")));
    }

    #[test]
    fn weight_u32_handles_non_finite() {
        let nan = Node {
            name: "nan".to_string(),
            weight: f64::NAN,
        };
        let inf = Node {
            name: "inf".to_string(),
            weight: f64::INFINITY,
        };
        assert_eq!(nan.weight_u32(), 0);
        assert_eq!(inf.weight_u32(), 0);
        assert_eq!(
            Node {
                name: "huge".to_string(),
                weight: f64::from(u32::MAX) * 2.0,
            }
            .weight_u32(),
            u32::MAX
        );
    }
}
