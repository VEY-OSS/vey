/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2024-2025 ByteDance and/or its affiliates.
 * Copyright 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;
use std::fs::DirBuilder;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

use anyhow::anyhow;
use log::{debug, warn};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::unix::SocketAddr;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::oneshot;

pub(super) struct LocalControllerImpl {
    listen_path: PathBuf,
    listener: UnixListener,
    create_time: i64,
    whitelist_users: BTreeSet<u32>,
}

impl LocalControllerImpl {
    fn new(listen_path: PathBuf) -> anyhow::Result<Self> {
        let current_uid = rustix::process::getuid().as_raw();

        let listener = UnixListener::bind(&listen_path).map_err(|e| {
            anyhow!(
                "bind to control socket {} failed: {e}",
                listen_path.display(),
            )
        })?;
        let metadata = listen_path.metadata().map_err(|e| {
            anyhow!(
                "failed to get metadata on control socket path {}: {}",
                listen_path.display(),
                e
            )
        })?;
        if !metadata.file_type().is_socket() || metadata.uid() != current_uid {
            return Err(anyhow!(
                "control socket path {} has been deleted",
                listen_path.display()
            ));
        }

        let mut whitelist_users = BTreeSet::new();
        whitelist_users.insert(current_uid);
        whitelist_users.insert(0);

        Ok(LocalControllerImpl {
            listen_path,
            listener,
            create_time: metadata.ctime(),
            whitelist_users,
        })
    }

    pub(super) fn listen_path(&self) -> String {
        self.listen_path.display().to_string()
    }

    pub(super) fn create_unique(_daemon_name: &str, daemon_group: &str) -> anyhow::Result<Self> {
        let socket_name = format!("{daemon_group}_{}.sock", std::process::id());
        let mut listen_path = crate::opts::control_dir();
        listen_path.push(Path::new(&socket_name));
        if listen_path.exists() {
            return Err(anyhow!(
                "control socket path {} already exists",
                listen_path.display()
            ));
        }
        check_then_finalize_path(&listen_path)?;

        debug!("setting up unique controller {}", listen_path.display());
        let controller = LocalControllerImpl::new(listen_path)?;
        debug!("unique controller created");
        Ok(controller)
    }

    pub(super) fn create_daemon(_daemon_name: &str, daemon_group: &str) -> anyhow::Result<Self> {
        let socket_name = if daemon_group.is_empty() {
            "_.sock".to_string()
        } else {
            format!("{daemon_group}.sock")
        };
        let mut listen_path = crate::opts::control_dir();
        listen_path.push(Path::new(&socket_name));
        match listen_path.symlink_metadata() {
            Ok(metadata) => {
                if !metadata.file_type().is_socket() {
                    return Err(anyhow!(
                        "control socket path {} exists but is not a socket",
                        listen_path.display()
                    ));
                }
                if metadata.uid() != rustix::process::getuid().as_raw() {
                    return Err(anyhow!(
                        "control socket path {} belongs to a different uid {}",
                        listen_path.display(),
                        metadata.uid()
                    ));
                }
                std::fs::remove_file(&listen_path)
                    .map_err(|e| anyhow!("failed to remove old {}: {e}", listen_path.display()))?;
            }
            Err(e) => {
                if e.kind() != io::ErrorKind::NotFound {
                    return Err(anyhow!(
                        "failed to check control socket {}",
                        listen_path.display()
                    ));
                }
            }
        }
        check_then_finalize_path(&listen_path)?;

        debug!("setting up daemon controller {}", listen_path.display());
        let controller = LocalControllerImpl::new(listen_path)?;
        debug!("daemon controller created");
        Ok(controller)
    }

    pub(super) async fn connect_to_daemon(
        _daemon_name: &str,
        daemon_group: &str,
    ) -> anyhow::Result<impl AsyncRead + AsyncWrite + use<>> {
        let socket_name = format!("{daemon_group}.sock");
        let mut socket_path = crate::opts::control_dir();
        socket_path.push(Path::new(&socket_name));

        UnixStream::connect(&socket_path).await.map_err(|e| {
            anyhow!(
                "failed to connect to control socket {}: {e:?}",
                socket_path.display()
            )
        })
    }

    pub(super) async fn into_running(
        self,
        mut quit_receiver: oneshot::Receiver<oneshot::Sender<Self>>,
    ) {
        loop {
            tokio::select! {
                biased;

                r = self.listener.accept() => {
                    match r {
                        Ok((stream, addr)) => {
                            self.handle_stream(stream, addr);
                        }
                        Err(e) => {
                            warn!("controller {} accept: {e}", self.listen_path.display());
                        }
                    }
                }
                r = &mut quit_receiver => {
                    if let Ok(v) = r {
                        let _ = v.send(self);
                    }
                    break;
                }
            }
        }
    }

    fn handle_stream(&self, stream: UnixStream, addr: SocketAddr) {
        if let Ok(ucred) = stream.peer_cred() {
            let peer_uid = ucred.uid();
            if !self.whitelist_users.contains(&peer_uid) {
                // only allow control message from root and current running user
                warn!(
                    "dropped ctl connection uid {peer_uid} pid {:?}",
                    ucred.pid()
                );
                return;
            }
            if let Some(addr) = addr.as_pathname() {
                debug!(
                    "new ctl client from {} uid {peer_uid} pid {:?}",
                    addr.display(),
                    ucred.pid()
                );
            } else {
                debug!("new ctl client from uid {peer_uid} pid {:?}", ucred.pid());
            }
        } else if let Some(addr) = addr.as_pathname() {
            debug!("new ctl client from {}", addr.display());
        } else {
            debug!("new ctl local control client");
        }

        let (r, w) = stream.into_split();
        super::ctl_handle(r, w);
    }
}

impl Drop for LocalControllerImpl {
    fn drop(&mut self) {
        if let Ok(metadata) = self.listen_path.symlink_metadata() {
            if !metadata.file_type().is_socket() {
                return;
            }
            if metadata.ctime() != self.create_time {
                return;
            }
            debug!("unlink socket file {}", self.listen_path.display());
            if let Err(e) = std::fs::remove_file(&self.listen_path) {
                warn!(
                    "failed to unlink control socket {}: {e}",
                    self.listen_path.display()
                );
            }
        }
    }
}

fn check_then_finalize_path(path: &Path) -> anyhow::Result<()> {
    if !path.has_root() {
        return Err(anyhow!(
            "control socket path {} is not absolute",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        debug!("creating control directory {}", parent.display());
        DirBuilder::new().recursive(true).create(parent)?;
    }

    Ok(())
}
