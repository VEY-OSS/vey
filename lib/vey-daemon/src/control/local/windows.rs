/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use anyhow::anyhow;
use log::{debug, warn};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::oneshot;

pub(super) struct LocalControllerImpl {
    pipe_name: String,
    server: Option<NamedPipeServer>,
}

impl LocalControllerImpl {
    fn create(pipe_name: String) -> anyhow::Result<Self> {
        let server = pipe_server_options(true).create(&pipe_name)?;
        Ok(LocalControllerImpl {
            pipe_name,
            server: Some(server),
        })
    }

    pub(super) fn listen_path(&self) -> String {
        self.pipe_name.clone()
    }

    pub(super) fn create_unique(daemon_name: &str, daemon_group: &str) -> anyhow::Result<Self> {
        let pipe_name = format!(
            r"\\.\pipe\{daemon_name}@{daemon_group}:{}",
            std::process::id()
        );
        LocalControllerImpl::create(pipe_name)
    }

    pub(super) fn create_daemon(daemon_name: &str, daemon_group: &str) -> anyhow::Result<Self> {
        let pipe_name = format!(r"\\.\pipe\{daemon_name}@{daemon_group}");
        LocalControllerImpl::create(pipe_name)
    }

    pub(super) async fn connect_to_daemon(
        daemon_name: &str,
        daemon_group: &str,
    ) -> anyhow::Result<impl AsyncRead + AsyncWrite + use<>> {
        let pipe_name = format!(r"\\.\pipe\{daemon_name}@{daemon_group}");

        tokio::net::windows::named_pipe::ClientOptions::new()
            .open(&pipe_name)
            .map_err(|e| anyhow!("failed to open connection to pipe {pipe_name}: {e:?}"))
    }

    pub(super) async fn into_running(
        mut self,
        mut quit_receiver: oneshot::Receiver<oneshot::Sender<Self>>,
    ) {
        loop {
            let pipe_name = self.pipe_name.clone();
            let mut quit_sender = None;
            let accepted = {
                let Some(server) = self.server.as_mut() else {
                    break;
                };
                tokio::select! {
                    biased;

                    r = server.connect() => {
                        match r {
                            Ok(()) => true,
                            Err(e) => {
                                warn!("controller {pipe_name} accept: {e}");
                                false
                            }
                        }
                    }
                    r = &mut quit_receiver => {
                        quit_sender = r.ok();
                        false
                    }
                }
            };
            if let Some(quit_sender) = quit_sender {
                let _ = quit_sender.send(self);
                break;
            }
            if !accepted {
                continue;
            }

            debug!("new ctl local control client");
            let Some(connected) = self.server.take() else {
                break;
            };
            // Open the next instance before handing off `connected`. Every
            // instance uses the same unlimited cap: a live client plus the
            // replacement listener does not fit in `max_instances(2)`, and a
            // mismatched cap fails `CreateNamedPipe`.
            match pipe_server_options(false).create(&self.pipe_name) {
                Ok(new_server) => {
                    self.server = Some(new_server);
                    let (r, w) = tokio::io::split(connected);
                    super::ctl_handle(r, w);
                }
                Err(e) => {
                    warn!("failed to re-open controller pipe {}: {e}", self.pipe_name);
                    let (r, w) = tokio::io::split(connected);
                    super::ctl_handle(r, w);
                    break;
                }
            }
        }
    }
}

/// `first` is only for the initial instance. Later instances must not set
/// `FILE_FLAG_FIRST_PIPE_INSTANCE`, and must not change `nMaxInstances`.
/// Leaving the cap at Tokio's unlimited default lets a connection stay open
/// while the next listener is created.
fn pipe_server_options(first: bool) -> ServerOptions {
    let mut opts = ServerOptions::new();
    opts.first_pipe_instance(first);
    opts
}
