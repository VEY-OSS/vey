/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use arc_swap::ArcSwapOption;

use vey_types::metrics::{MetricTagMap, NodeName};
use vey_types::stats::StatId;

use crate::escape::EscaperStats;

pub(super) struct DummyDenyEscaperStats {
    name: NodeName,
    id: StatId,
    extra_metrics_tags: Arc<ArcSwapOption<MetricTagMap>>,
}

impl DummyDenyEscaperStats {
    pub(super) fn new(name: &NodeName) -> Self {
        DummyDenyEscaperStats {
            name: name.clone(),
            id: StatId::new_unique(),
            extra_metrics_tags: Arc::new(ArcSwapOption::new(None)),
        }
    }

    pub(super) fn set_extra_tags(&self, tags: Option<Arc<MetricTagMap>>) {
        self.extra_metrics_tags.store(tags);
    }
}

impl EscaperStats for DummyDenyEscaperStats {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn stat_id(&self) -> StatId {
        self.id
    }

    fn load_extra_tags(&self) -> Option<Arc<MetricTagMap>> {
        self.extra_metrics_tags.load_full()
    }

    fn share_extra_tags(&self) -> &Arc<ArcSwapOption<MetricTagMap>> {
        &self.extra_metrics_tags
    }

    fn connection_attempted(&self) -> u64 {
        0
    }

    fn connection_established(&self) -> u64 {
        0
    }
}
