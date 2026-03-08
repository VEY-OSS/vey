/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 */

use std::sync::Arc;

use vey_types::log::LogStats;
use vey_types::stats::StatId;

pub(crate) struct LoggerStats {
    id: StatId,
    name: String,
    inner: Arc<LogStats>,
}

impl LoggerStats {
    pub(crate) fn new(name: &str, inner: Arc<LogStats>) -> Self {
        LoggerStats {
            id: StatId::new_unique(),
            name: name.to_string(),
            inner,
        }
    }

    pub(crate) fn stat_id(&self) -> StatId {
        self.id
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn inner(&self) -> &Arc<LogStats> {
        &self.inner
    }
}
