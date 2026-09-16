/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::sync::Arc;

use super::{
    IcapClientConnection, IcapConnectionPool, IcapConnector, IcapServiceConfig, PoolMaintainer,
};

use crate::options::IcapServiceOptions;

pub struct IcapServiceClient {
    pub(crate) config: Arc<IcapServiceConfig>,
    pub(crate) partial_request_header: Vec<u8>,
    conn_pool: Arc<IcapConnectionPool>,
}

impl IcapServiceClient {
    pub fn new(config: Arc<IcapServiceConfig>) -> anyhow::Result<Self> {
        let connector = Arc::new(IcapConnector::new(config.clone())?);
        let conn_pool = Arc::new(IcapConnectionPool::new(config.clone(), connector));
        let check_interval = config.connection_pool.check_interval();

        let maintainer = PoolMaintainer::new(Arc::downgrade(&conn_pool), check_interval);

        tokio::spawn(maintainer.into_running());

        let partial_request_header = config.build_request_header();
        Ok(IcapServiceClient {
            config,
            partial_request_header,
            conn_pool,
        })
    }

    pub async fn fetch_connection(
        &self,
    ) -> anyhow::Result<(IcapClientConnection, Arc<IcapServiceOptions>)> {
        let mut conn = self.conn_pool.get().await?;
        let options = self.conn_pool.get_options();
        conn.mark_io_inuse();
        Ok((conn, options))
    }

    pub fn save_connection(&self, conn: IcapClientConnection) {
        self.conn_pool.try_put(conn);
    }
}
