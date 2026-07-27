/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Mutex;

use vey_types::stats::GlobalStatsMap;

pub fn move_ht<T>(in_ht_lock: &Mutex<GlobalStatsMap<T>>, out_ht_lock: &Mutex<GlobalStatsMap<T>>) {
    let mut tmp_req_map = GlobalStatsMap::new();
    let mut in_req_map = in_ht_lock.lock().unwrap();
    for (k, v) in in_req_map.drain() {
        tmp_req_map.insert(k, v);
    }
    drop(in_req_map); // drop early

    if !tmp_req_map.is_empty() {
        let mut out_req_map = out_ht_lock.lock().unwrap();
        for (k, v) in tmp_req_map.drain() {
            out_req_map.insert(k, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use vey_types::stats::StatId;

    #[test]
    fn move_ht_transfers_entries() {
        let in_ht = Mutex::new(GlobalStatsMap::<u32>::new());
        let out_ht = Mutex::new(GlobalStatsMap::<u32>::new());
        let id = StatId::new_unique();

        in_ht.lock().unwrap().insert(id, 42);
        move_ht(&in_ht, &out_ht);

        assert!(in_ht.lock().unwrap().is_empty());
        assert_eq!(*out_ht.lock().unwrap().get_or_insert_with(id, || 0), 42);
    }

    #[test]
    fn move_ht_noop_when_source_empty() {
        let in_ht = Mutex::new(GlobalStatsMap::<u32>::new());
        let out_ht = Mutex::new(GlobalStatsMap::<u32>::new());
        let id = StatId::new_unique();
        out_ht.lock().unwrap().insert(id, 7);

        move_ht(&in_ht, &out_ht);
        assert_eq!(*out_ht.lock().unwrap().get_or_insert_with(id, || 0), 7);
    }
}
