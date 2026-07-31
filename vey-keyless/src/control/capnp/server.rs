/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::rc::Rc;
use std::str::FromStr;

use anyhow::anyhow;

use vey_types::metrics::{MetricTagName, MetricTagValue, NodeName};

use vey_keyless_proto::server_capnp::server_control;

use super::set_operation_result;
use crate::serve::ArcKeyServer;

pub(super) struct ServerControlImpl {
    server: ArcKeyServer,
}

impl ServerControlImpl {
    pub(super) fn new_client(name: &str) -> anyhow::Result<server_control::Client> {
        let name =
            NodeName::from_str(name).map_err(|e| anyhow!("invalid server name {name}: {e}"))?;
        let server = crate::serve::get_server(&name)?;
        Ok(capnp_rpc::new_client(ServerControlImpl { server }))
    }

    fn do_add_metrics_tag(&self, name: &str, value: &str) -> anyhow::Result<()> {
        let name =
            MetricTagName::from_str(name).map_err(|e| anyhow!("invalid metrics tag name: {e}"))?;
        let value = MetricTagValue::from_str(value)
            .map_err(|e| anyhow!("invalid metrics tag value: {e}"))?;
        self.server.add_dynamic_metrics_tag(name, value);
        Ok(())
    }
}

impl server_control::Server for ServerControlImpl {
    async fn status(
        self: Rc<Self>,
        _params: server_control::StatusParams,
        mut results: server_control::StatusResults,
    ) -> capnp::Result<()> {
        let mut builder = results.get().init_status();
        match self.server.get_server_stats() {
            Some(stats) => {
                builder.set_online(stats.is_online());
                builder.set_alive_task_count(stats.get_alive_count());
                builder.set_total_task_count(stats.get_task_total());
            }
            None => {
                // port servers do not track key operation tasks
                let stats = self.server.get_listen_stats();
                builder.set_online(stats.is_running());
                builder.set_alive_task_count(0);
                builder.set_total_task_count(stats.accepted());
            }
        }
        Ok(())
    }

    async fn add_metrics_tag(
        self: Rc<Self>,
        params: server_control::AddMetricsTagParams,
        mut results: server_control::AddMetricsTagResults,
    ) -> capnp::Result<()> {
        let name = params.get()?.get_name()?.to_str()?;
        let value = params.get()?.get_value()?.to_str()?;

        let r = self.do_add_metrics_tag(name, value);
        set_operation_result(results.get().init_result(), r);
        Ok(())
    }

    async fn get_listen_addr(
        self: Rc<Self>,
        _params: server_control::GetListenAddrParams,
        mut results: server_control::GetListenAddrResults,
    ) -> capnp::Result<()> {
        let addr = self
            .server
            .listen_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_default();
        results.get().set_addr(addr.as_str());
        Ok(())
    }
}
