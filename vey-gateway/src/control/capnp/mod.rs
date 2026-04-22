/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use vey_gateway_proto::proc_capnp::proc_control;

mod common;
use common::set_operation_result;
mod proc;

mod backend;
mod server;

pub fn stop_working_thread() {
    vey_daemon::control::capnp::stop_working_thread();
}

fn build_capnp_client() -> capnp::capability::Client {
    let control_client: proc_control::Client = capnp_rpc::new_client(proc::ProcControlImpl);
    control_client.client
}

pub async fn spawn_working_thread() -> anyhow::Result<std::thread::JoinHandle<()>> {
    vey_daemon::control::capnp::spawn_working_thread(&build_capnp_client).await
}
