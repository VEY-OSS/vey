/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2024-2025 ByteDance and/or its affiliates.
 */

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use foldhash::HashMap;

use vey_types::metrics::NodeName;

use crate::config::escaper::EgressUpstream;

#[derive(Clone, Debug, Default)]
pub(crate) struct EgressPathSelection {
    context_kv: BTreeMap<String, String>,
    integer_index: HashMap<NodeName, usize>,
    string_index: HashMap<NodeName, String>,
    upstream: Arc<Mutex<HashMap<NodeName, Arc<EgressUpstream>>>>,
    json: HashMap<NodeName, serde_json::Value>,
}

impl EgressPathSelection {
    pub(crate) fn with_context_kv(context_kv: BTreeMap<String, String>) -> Self {
        EgressPathSelection {
            context_kv,
            ..Default::default()
        }
    }

    pub(crate) fn context_kv(&self) -> &BTreeMap<String, String> {
        &self.context_kv
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.context_kv.is_empty()
            && self.integer_index.is_empty()
            && self.string_index.is_empty()
            && self.upstream.lock().unwrap().is_empty()
            && self.json.is_empty()
    }

    pub(crate) fn set_number_id(&mut self, escaper: NodeName, id: usize) {
        self.integer_index.insert(escaper, id);
    }

    /// get the selection id
    /// `len` should not be zero
    /// the returned id will be in range 0..len
    pub(crate) fn select_number_id(&self, escaper: &NodeName, len: usize) -> Option<usize> {
        let id = self.integer_index.get(escaper)?;
        let id = *id;
        let i = if id == 0 {
            len - 1
        } else if id <= len {
            id - 1
        } else {
            (id - 1) % len
        };
        Some(i)
    }

    pub(crate) fn set_string_id(&mut self, escaper: NodeName, id: String) {
        self.string_index.insert(escaper, id);
    }

    pub(crate) fn select_string_id(&self, escaper: &NodeName) -> Option<&str> {
        self.string_index.get(escaper).map(|s| s.as_str())
    }

    pub(crate) fn set_upstream(&self, escaper: NodeName, ups: EgressUpstream) {
        let mut upstream_map = self.upstream.lock().unwrap();
        upstream_map.insert(escaper, Arc::new(ups));
    }

    pub(crate) fn select_upstream(&self, escaper: &NodeName) -> Option<Arc<EgressUpstream>> {
        let upstream_map = self.upstream.lock().unwrap();
        upstream_map.get(escaper).cloned()
    }

    pub(crate) fn set_json_value(&mut self, escaper: NodeName, v: serde_json::Value) {
        self.json.insert(escaper, v);
    }

    pub(crate) fn select_json_value(&self, escaper: &NodeName) -> Option<&serde_json::Value> {
        self.json.get(escaper)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_index() {
        const LENGTH: usize = 30;
        const ESCAPER: NodeName = NodeName::new_static("abcd");

        let mut egress_path = EgressPathSelection::default();
        egress_path.set_number_id(ESCAPER.clone(), 1);
        assert_eq!(Some(0), egress_path.select_number_id(&ESCAPER, LENGTH));

        egress_path.set_number_id(ESCAPER.clone(), 2);
        assert_eq!(Some(1), egress_path.select_number_id(&ESCAPER, LENGTH));

        egress_path.set_number_id(ESCAPER.clone(), 30);
        assert_eq!(Some(29), egress_path.select_number_id(&ESCAPER, LENGTH));

        egress_path.set_number_id(ESCAPER.clone(), 0);
        assert_eq!(Some(29), egress_path.select_number_id(&ESCAPER, LENGTH));

        egress_path.set_number_id(ESCAPER.clone(), 31);
        assert_eq!(Some(0), egress_path.select_number_id(&ESCAPER, LENGTH));

        egress_path.set_number_id(ESCAPER.clone(), 60);
        assert_eq!(Some(29), egress_path.select_number_id(&ESCAPER, LENGTH));

        egress_path.set_number_id(ESCAPER.clone(), 61);
        assert_eq!(Some(0), egress_path.select_number_id(&ESCAPER, LENGTH));
    }
}
