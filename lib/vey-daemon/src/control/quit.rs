/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Mutex;
use std::time::Duration;

use anyhow::anyhow;
use log::{info, warn};
use tokio::sync::{mpsc, oneshot};

static DAEMON_QUIT_CHANNEL: Mutex<Option<mpsc::Sender<Command>>> = Mutex::new(None);

enum Command {
    StartGracefulShutdown,
    ReleaseController(oneshot::Sender<bool>),
    CancelGracefulShutdown(oneshot::Sender<bool>),
}

async fn send_command(msg: Command) {
    let mut channel = DAEMON_QUIT_CHANNEL.lock().unwrap().clone();
    if let Some(channel) = channel.take() {
        let _ = channel.send(msg).await;
    }
}

pub async fn start_graceful_shutdown() {
    send_command(Command::StartGracefulShutdown).await;
}

pub async fn release_controller() -> anyhow::Result<()> {
    let (sender, receiver) = oneshot::channel();
    send_command(Command::ReleaseController(sender)).await;
    match receiver.await {
        Ok(true) => Ok(()),
        Ok(false) => Err(anyhow!("not in graceful shutdown state")),
        Err(_) => Err(anyhow!("channel closed unexpectedly")),
    }
}

pub async fn cancel_graceful_shutdown() -> anyhow::Result<()> {
    let (sender, receiver) = oneshot::channel();
    send_command(Command::CancelGracefulShutdown(sender)).await;
    match receiver.await {
        Ok(true) => Ok(()),
        Ok(false) => Err(anyhow!("failed to resume controller")),
        Err(_) => Err(anyhow!("channel closed unexpectedly")),
    }
}

pub fn trigger_force_shutdown() {
    let mut channel = DAEMON_QUIT_CHANNEL.lock().unwrap();
    *channel = None;
}

pub trait QuitAction: Default {
    /// Release the daemon controller so a new process can bind it.
    ///
    /// The quit loop calls this before either shutdown path, and skips it when
    /// the controller is already released. [`resume_controller`](Self::resume_controller)
    /// undoes it when graceful shutdown is cancelled.
    fn release_controller(&self) -> impl Future<Output = ()> + Send;

    /// Bind the daemon controller again after graceful shutdown is cancelled.
    fn resume_controller(&self, program_name: String) -> anyhow::Result<()>;

    /// Stop listeners and other runtime work, without waiting for in-flight tasks.
    ///
    /// Called once on both the graceful and force paths, after the daemon
    /// controller has been released.
    fn start_shutdown(&self) -> impl Future<Output = ()> + Send;

    /// Wait until in-flight tasks finish.
    ///
    /// Used only by graceful shutdown. A quit signal drops this future and
    /// continues at [`shutdown`](Self::shutdown). The default returns immediately.
    fn wait_graceful_shutdown(&self) -> impl Future<Output = ()> + Send {
        std::future::ready(())
    }

    /// Abort the unique controller so the process can leave its main loop.
    ///
    /// Called after [`start_shutdown`](Self::start_shutdown). Graceful shutdown
    /// reaches it when the wait finishes or a quit signal arrives during the wait.
    /// Force shutdown reaches it immediately after `start_shutdown`.
    fn shutdown(&self) -> impl Future<Output = ()> + Send;

    /// Spawn the quit loop on the current runtime.
    fn tokio_spawn_run(program_name: &str)
    where
        Self: Send + Sync + 'static,
    {
        let actor = Self::default();
        let program_name = program_name.to_owned();
        tokio::spawn(QuitLoop::new(program_name, actor).run());
    }
}

struct QuitLoop<T: QuitAction> {
    action: T,
    program_name: String,
    graceful_wait: Duration,
    controller_released: bool,
    msg_receiver: mpsc::Receiver<Command>,
}

impl<T: QuitAction> QuitLoop<T> {
    fn new(program_name: String, action: T) -> Self {
        let (msg_sender, msg_receiver) = mpsc::channel(4);
        set_daemon_quit_channel(msg_sender);

        QuitLoop {
            action,
            program_name,
            graceful_wait: crate::runtime::config::get_server_offline_delay(),
            controller_released: false,
            msg_receiver,
        }
    }

    async fn run(mut self) {
        'outer: while let Some(msg) = self.msg_receiver.recv().await {
            match msg {
                Command::StartGracefulShutdown => {
                    // this should be sent by an init, which doesn't know it's a restart or stop
                    info!("received StartGracefulShutdown request");
                    let mut is_restart = false;
                    loop {
                        info!(
                            "will start graceful shutdown after {:?}",
                            self.graceful_wait
                        );
                        match tokio::time::timeout(self.graceful_wait, self.msg_receiver.recv())
                            .await
                        {
                            Ok(Some(Command::StartGracefulShutdown)) => {
                                if is_restart {
                                    // the new process tell us to stop
                                    break;
                                }
                            }
                            Ok(Some(Command::ReleaseController(finish_sender))) => {
                                // this should be sent by the new process, now we know it's a restart
                                self.release_controller().await;
                                let _ = finish_sender.send(true);
                                is_restart = true;
                            }
                            Ok(Some(Command::CancelGracefulShutdown(finish_sender))) => {
                                // the new process tell us to stop
                                info!("received CancelGracefulShutdown request");
                                let resumed = self.resume_controller().await;
                                let _ = finish_sender.send(resumed);
                                info!("graceful shutdown canceled");
                                continue 'outer;
                            }
                            Ok(None) => break 'outer,
                            Err(_) => {
                                if is_restart {
                                    // the new process may be failed to start, so we resume
                                    info!(
                                        "timeout to wait StartGracefulShutdown request, will resume"
                                    );
                                    self.resume_controller().await;
                                    info!("graceful shutdown canceled");
                                    continue 'outer;
                                } else {
                                    // treat timeout to stop
                                    info!("timeout to wait new controller request, will stop");
                                    self.release_controller().await;
                                    break;
                                }
                            }
                        }
                    }
                    self.graceful_shutdown().await;
                    return;
                }
                Command::ReleaseController(finish_sender) => {
                    // this should be sent by the new process, which knows it's a restart
                    info!("received ReleaseController request");
                    self.release_controller().await;
                    let _ = finish_sender.send(true);
                    loop {
                        info!(
                            "will start graceful shutdown after {:?}",
                            self.graceful_wait
                        );
                        match tokio::time::timeout(self.graceful_wait, self.msg_receiver.recv())
                            .await
                        {
                            Ok(Some(Command::StartGracefulShutdown)) => {
                                // the new process tell us to stop
                                break;
                            }
                            Ok(Some(Command::ReleaseController(finish_sender))) => {
                                let _ = finish_sender.send(true);
                            }
                            Ok(Some(Command::CancelGracefulShutdown(finish_sender))) => {
                                // the new process tell us to resume
                                info!("received CancelGracefulShutdown request");
                                let resumed = self.resume_controller().await;
                                let _ = finish_sender.send(resumed);
                                info!("graceful shutdown canceled");
                                continue 'outer;
                            }
                            Ok(None) => break 'outer,
                            Err(_) => {
                                // the new process may be failed to start, so we consume
                                info!("timeout to wait StartGracefulShutdown request, will resume");
                                self.resume_controller().await;
                                info!("graceful shutdown canceled");
                                continue 'outer;
                            }
                        }
                    }
                    self.graceful_shutdown().await;
                    return;
                }
                Command::CancelGracefulShutdown(finish_sender) => {
                    let _ = finish_sender.send(true);
                }
            }
        }

        self.release_controller().await;
        info!("start force shutdown");
        self.action.start_shutdown().await;
        self.action.shutdown().await;
        clear_daemon_quit_channel();
    }

    async fn graceful_shutdown(&mut self) {
        info!("start graceful shutdown now");
        self.action.start_shutdown().await;
        info!("waiting for graceful shutdown");

        let action = &self.action;
        let msg_receiver = &mut self.msg_receiver;

        tokio::pin! {
            let waiting = action.wait_graceful_shutdown();
        }

        loop {
            tokio::select! {
                _ = &mut waiting => break,
                msg = msg_receiver.recv() => match msg {
                    // trigger_force_shutdown drops the sender; recv then returns None.
                    None => {
                        info!("received quit request, shutdown now");
                        break;
                    }
                    Some(Command::StartGracefulShutdown) => {}
                    Some(Command::ReleaseController(finish_sender)) => {
                        let _ = finish_sender.send(true);
                    }
                    Some(Command::CancelGracefulShutdown(finish_sender)) => {
                        info!("received CancelGracefulShutdown request");
                        let _ = finish_sender.send(false);
                        info!("graceful shutdown already started and can not be canceled");
                    }
                },
            }
        }

        action.shutdown().await;
    }

    async fn release_controller(&mut self) {
        if self.controller_released {
            return;
        }
        self.action.release_controller().await;
        self.controller_released = true;
        info!("daemon controller released");
    }

    async fn resume_controller(&mut self) -> bool {
        if self.controller_released {
            match self.action.resume_controller(self.program_name.clone()) {
                Ok(_) => {
                    self.controller_released = false;
                    info!("daemon controller resumed");
                    true
                }
                Err(e) => {
                    warn!("failed to resume daemon controller: {e}");
                    false
                }
            }
        } else {
            true
        }
    }
}

fn set_daemon_quit_channel(msg_sender: mpsc::Sender<Command>) {
    let mut channel = DAEMON_QUIT_CHANNEL.lock().unwrap();
    *channel = Some(msg_sender);
}

fn clear_daemon_quit_channel() {
    let mut channel = DAEMON_QUIT_CHANNEL.lock().unwrap();
    let _ = channel.take();
}
