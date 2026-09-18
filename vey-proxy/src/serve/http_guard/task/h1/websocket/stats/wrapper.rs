/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use smallvec::SmallVec;

use vey_io_ext::{
    ArcLimitedReaderStats, ArcLimitedWriterStats, LimitedReaderStats, LimitedWriterStats,
};

use crate::auth::{UserTrafficStats, UserTrafficStatsList};
use crate::module::websocket::WebSocketTaskStats;
use crate::serve::http_guard::HttpGuardServerStats;

trait WebSocketTaskCltStatsWrapper {
    fn add_read_bytes(&self, size: u64);
    fn add_write_bytes(&self, size: u64);
}

type ArcWebSocketTaskCltStatsWrapper = Arc<dyn WebSocketTaskCltStatsWrapper + Send + Sync>;

impl WebSocketTaskCltStatsWrapper for UserTrafficStats {
    fn add_read_bytes(&self, size: u64) {
        self.io.websocket.add_in_bytes(size);
    }

    fn add_write_bytes(&self, size: u64) {
        self.io.websocket.add_out_bytes(size);
    }
}

#[derive(Clone)]
pub(crate) struct WebSocketTaskCltWrapperStats {
    server: Arc<HttpGuardServerStats>,
    task: Arc<WebSocketTaskStats>,
    others: SmallVec<[ArcWebSocketTaskCltStatsWrapper; 4]>,
}

impl WebSocketTaskCltWrapperStats {
    pub(crate) fn new(server: &Arc<HttpGuardServerStats>, task: &Arc<WebSocketTaskStats>) -> Self {
        WebSocketTaskCltWrapperStats {
            server: Arc::clone(server),
            task: Arc::clone(task),
            others: SmallVec::new(),
        }
    }

    pub(crate) fn push_user_io_stats(&mut self, all: UserTrafficStatsList) {
        for s in all {
            self.others.push(s);
        }
    }

    pub(crate) fn split(self) -> (ArcLimitedReaderStats, ArcLimitedWriterStats) {
        let s = Arc::new(self);
        (s.clone(), s)
    }
}

impl LimitedReaderStats for WebSocketTaskCltWrapperStats {
    fn add_read_bytes(&self, size: usize) {
        let size = size as u64;
        self.task.clt.read.add_bytes(size);
        self.server.io_http.add_in_bytes(size);
        self.others.iter().for_each(|s| s.add_read_bytes(size));
    }
}

impl LimitedWriterStats for WebSocketTaskCltWrapperStats {
    fn add_write_bytes(&self, size: usize) {
        let size = size as u64;
        self.task.clt.write.add_bytes(size);
        self.server.io_http.add_out_bytes(size);
        self.others.iter().for_each(|s| s.add_write_bytes(size));
    }
}
