/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use log::{debug, warn};
use tokio::runtime::Handle;
use tokio::time::Instant;

use vey_cert_agent::Request;
use vey_daemon::listen::{RespondUdpRuntime, UdpResponder};
use vey_daemon::server::ServerReloadCommand;
use vey_std_ext::time::DurationExt;

pub mod config;

mod build;

pub mod opts;
use opts::ProcArgs;

mod stat;

mod backend;
use backend::{BackendStats, OpensslBackend};

mod frontend;
use frontend::{Frontend, FrontendStats, GeneratedData};

struct BackendRequest {
    user_req: Request,
    peer: SocketAddr,
    responder: UdpResponder,
    recv_time: Instant,
}

struct BackendResponse {
    user_req: Request,
    generated: GeneratedData,
    peer: SocketAddr,
    responder: UdpResponder,
    recv_time: Instant,
}

impl BackendRequest {
    fn into_response(self, generated: GeneratedData) -> BackendResponse {
        BackendResponse {
            user_req: self.user_req,
            generated,
            peer: self.peer,
            responder: self.responder,
            recv_time: self.recv_time,
        }
    }
}

impl BackendResponse {
    fn duration(&self) -> u64 {
        self.recv_time.elapsed().as_nanos_u64()
    }
}

pub async fn run(proc_args: &ProcArgs) -> anyhow::Result<()> {
    let (req_sender, req_receiver) = kanal::bounded_async::<BackendRequest>(1024);
    let (rsp_sender, rsp_receiver) = kanal::bounded_async::<BackendResponse>(1024);

    let backend_config =
        config::get_backend_config().ok_or_else(|| anyhow!("no backend config available"))?;
    let backend_stats = Arc::new(BackendStats::default());

    let (duration_recorder, duration_stats) = backend_config.duration_stats.build_spawned(None);

    let workers = vey_daemon::runtime::worker::foreach(|h| {
        let backend = OpensslBackend::new(&backend_config, &backend_stats)
            .context(format!("failed to build backend for worker {}", h.id))?;
        backend.spawn(&h.handle, h.id, req_receiver.clone(), rsp_sender.clone());
        Ok::<(), anyhow::Error>(())
    })?;
    if workers < 1 {
        let backend = OpensslBackend::new(&backend_config, &backend_stats)
            .context("failed to build backend for main runtime")?;
        backend.spawn(&Handle::current(), 0, req_receiver, rsp_sender);
    } else {
        drop(req_receiver);
        drop(rsp_sender);
    }

    let frontend_stats = Arc::new(FrontendStats::default());
    let frontend = Frontend::new(frontend_stats.clone(), req_sender);

    if let Some(stats_config) = vey_daemon::stat::config::get_global_stat_config() {
        stat::spawn_working_thread(
            stats_config,
            backend_stats,
            duration_stats,
            frontend.stats(),
        )?;
    }

    let rsp_stats = frontend_stats;
    let rsp_task = tokio::spawn(async move {
        while let Ok(rsp) = rsp_receiver.recv().await {
            frontend::send_response(&rsp_stats, &duration_recorder, rsp).await;
        }
    });

    let reload_sender = ServerReloadCommand::new_sender();
    let tasks = {
        let mut runtime = RespondUdpRuntime::new(frontend, proc_args.listen_config().clone());
        runtime.run_all_instances(false, &reload_sender)?
    };

    if let Err(e) = tokio::signal::ctrl_c().await {
        warn!("failed to recv Ctrl-C signal: {e}");
    }
    debug!("received Ctrl-C signal, start shutdown now");
    let _ = reload_sender.send(ServerReloadCommand::QuitRuntime);
    for task in tasks {
        if let Err(e) = task.handle.await {
            warn!("dcgen respond task ended with error: {e}");
        }
    }
    let _ = rsp_task.await;
    debug!("all requests served, quit now");
    Ok(())
}
