/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use std::future::poll_fn;

use anyhow::anyhow;
use log::info;
use tokio::signal::unix::{SignalKind, signal};

use super::AsyncSignalAction;

pub fn register_quit<QUIT>(do_quit: QUIT) -> anyhow::Result<()>
where
    QUIT: AsyncSignalAction + Send + 'static,
{
    let mut quit_sig = signal(SignalKind::quit())
        .map_err(|e| anyhow!("failed to create SIGQUIT listener: {e}"))?;
    tokio::spawn(async move {
        if poll_fn(|cx| quit_sig.poll_recv(cx)).await.is_some() {
            info!("got quit signal");
            do_quit.run().await;
        }
    });

    let mut int_sig = signal(SignalKind::interrupt())
        .map_err(|e| anyhow!("failed to create SIGINT listener: {e}"))?;
    tokio::spawn(async move {
        if poll_fn(|cx| int_sig.poll_recv(cx)).await.is_some() {
            info!("got quit signal");
            do_quit.run().await;
        }
    });

    Ok(())
}

pub fn register_offline<OFFLINE>(go_offline: OFFLINE) -> anyhow::Result<()>
where
    OFFLINE: AsyncSignalAction + Send + 'static,
{
    let mut term_sig = signal(SignalKind::terminate())
        .map_err(|e| anyhow!("failed to create SIGTERM listener: {e}"))?;
    tokio::spawn(async move {
        // Graceful shutdown can be cancelled and the process keeps running.
        // A later SIGTERM has to reach the quit loop again.
        loop {
            if poll_fn(|cx| term_sig.poll_recv(cx)).await.is_none() {
                break;
            }
            info!("got offline signal");
            go_offline.run().await;
        }
    });

    Ok(())
}

pub fn register_reload<RELOAD>(call_reload: RELOAD) -> anyhow::Result<()>
where
    RELOAD: AsyncSignalAction + Send + 'static,
{
    let mut hup_sig = signal(SignalKind::hangup())
        .map_err(|e| anyhow!("failed to create SIGHUP listener: {e}"))?;
    tokio::spawn(async move {
        loop {
            if poll_fn(|cx| hup_sig.poll_recv(cx)).await.is_none() {
                break;
            }
            info!("got reload signal");
            call_reload.run().await;
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use super::{AsyncSignalAction, register_offline};

    const CHILD_ENV: &str = "VEY_DAEMON_SIGNAL_CHILD";
    const LOG_ENV: &str = "VEY_DAEMON_SIGNAL_LOG";

    static HITS: AtomicUsize = AtomicUsize::new(0);

    #[derive(Clone, Copy)]
    struct CountOffline;

    impl AsyncSignalAction for CountOffline {
        async fn run(&self) {
            let n = HITS.fetch_add(1, Ordering::SeqCst) + 1;
            let path = std::env::var(LOG_ENV).expect("signal log path");
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(path)
                .expect("open signal log");
            writeln!(file, "{n}").expect("write signal log");
            file.flush().expect("flush signal log");
        }
    }

    #[test]
    fn terminate_signal_is_received_again() {
        if std::env::var_os(CHILD_ENV).is_some() {
            run_child();
            return;
        }

        let log_path =
            std::env::temp_dir().join(format!("vey-daemon-signal-{}", std::process::id()));
        let _cleanup = LogCleanup(&log_path);
        std::fs::write(&log_path, "").unwrap();

        let child = Command::new(std::env::current_exe().unwrap())
            .arg("terminate_signal_is_received_again")
            .env(CHILD_ENV, "1")
            .env(LOG_ENV, &log_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut child = RunningChild::new(child);

        wait_for(&log_path, child.get_mut(), "ready");
        signal_child(child.get_mut(), rustix::process::Signal::TERM);
        wait_for(&log_path, child.get_mut(), "\n1\n");
        signal_child(child.get_mut(), rustix::process::Signal::TERM);
        wait_for(&log_path, child.get_mut(), "\n2\n");

        let status = child.wait();
        assert!(status.success(), "signal child failed: {status}");
    }

    fn run_child() {
        let path = std::env::var(LOG_ENV).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            register_offline(CountOffline).unwrap();
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            writeln!(file, "ready").unwrap();
            file.flush().unwrap();

            let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
            loop {
                if HITS.load(Ordering::SeqCst) >= 2 {
                    break;
                }
                if tokio::time::Instant::now() >= deadline {
                    std::process::exit(1);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
    }

    fn wait_for(path: &Path, child: &mut Child, needle: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let text = std::fs::read_to_string(path).unwrap_or_default();
            if text.contains(needle) {
                return;
            }
            match child.try_wait() {
                Ok(Some(status)) => panic!("signal child exited early: {status}; log={text}"),
                Ok(None) => {}
                Err(e) => panic!("failed to poll signal child: {e}"),
            }
            if Instant::now() >= deadline {
                panic!("timed out waiting for {needle}; log={text}");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn signal_child(child: &Child, signal: rustix::process::Signal) {
        let pid = i32::try_from(child.id()).expect("child pid fits in i32");
        let pid = rustix::process::Pid::from_raw(pid).expect("child pid");
        rustix::process::kill_process(pid, signal).expect("signal child");
    }

    struct RunningChild(Option<Child>);

    impl RunningChild {
        fn new(child: Child) -> Self {
            Self(Some(child))
        }

        fn get_mut(&mut self) -> &mut Child {
            self.0.as_mut().expect("child still running")
        }

        fn wait(mut self) -> std::process::ExitStatus {
            self.0.take().expect("child still running").wait().unwrap()
        }
    }

    impl Drop for RunningChild {
        fn drop(&mut self) {
            if let Some(mut child) = self.0.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    struct LogCleanup<'a>(&'a Path);

    impl Drop for LogCleanup<'_> {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.0);
        }
    }
}
