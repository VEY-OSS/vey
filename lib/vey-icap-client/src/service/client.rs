/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use super::{IcapClientConnection, IcapConnectionPool, IcapConnector, IcapServiceConfig};
use crate::options::IcapServiceOptions;

pub struct IcapServiceClient {
    pub(crate) config: Arc<IcapServiceConfig>,
    pub(crate) partial_request_header: Vec<u8>,
    conn_pool: Arc<IcapConnectionPool>,
}

impl IcapServiceClient {
    pub fn new(config: Arc<IcapServiceConfig>) -> anyhow::Result<Self> {
        Self::with_worker_count(config, 1)
    }

    /// `worker_count` is the number of unaided workers. Each worker gets its own
    /// idle lane. Zero means the process has no unaided workers, so one shared
    /// lane is enough.
    pub fn with_worker_count(
        config: Arc<IcapServiceConfig>,
        worker_count: usize,
    ) -> anyhow::Result<Self> {
        let lane_count = worker_count.max(1);
        let connector = Arc::new(IcapConnector::new(config.clone())?);
        let conn_pool = Arc::new(IcapConnectionPool::new(
            config.clone(),
            connector,
            lane_count,
        ));
        // The lane maintainer starts on the first checkout or return, which runs
        // on the worker that owns the sockets. Starting it here would bind those
        // sockets to whatever runtime loaded the config.

        let partial_request_header = config.build_request_header();
        Ok(IcapServiceClient {
            config,
            partial_request_header,
            conn_pool,
        })
    }

    /// `worker_id` selects the unaided-worker lane. `None` and out-of-range ids
    /// use lane 0.
    pub async fn fetch_connection(
        &self,
        worker_id: Option<usize>,
    ) -> anyhow::Result<(IcapClientConnection, Arc<IcapServiceOptions>)> {
        self.conn_pool.ensure_worker(worker_id);
        let mut conn = self.conn_pool.get(worker_id).await?;
        let options = self.conn_pool.get_options();
        conn.mark_io_inuse();
        Ok((conn, options))
    }

    pub fn save_connection(&self, conn: IcapClientConnection) {
        self.conn_pool.ensure_lane(conn.lane());
        self.conn_pool.try_put(conn);
    }
}
