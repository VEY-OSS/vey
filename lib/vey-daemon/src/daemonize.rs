/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::anyhow;
use log::debug;

use crate::opts::DaemonArgs;

pub fn check_enter(args: &DaemonArgs) -> anyhow::Result<()> {
    if args.daemon_mode {
        enter(args)?;
        debug!("enter daemon mode");
    }
    Ok(())
}

fn enter(args: &DaemonArgs) -> anyhow::Result<()> {
    // `daemon(0, 0)` chdirs to `/`. Resolve and open a relative pid path
    // against the current directory first. The pid itself is written after
    // `daemon` returns, because that call forks and the daemon has a new id.
    let pidfile = match &args.pid_file {
        Some(path) => Some(open_pid_file(path)?),
        None => None,
    };

    unsafe {
        let r = libc::daemon(0, 0);
        if r != 0 {
            let e = std::io::Error::last_os_error();
            return Err(anyhow!("daemon(0, 0) failed: {e}"));
        }
    }

    if let Some((path, mut pidfile)) = pidfile {
        let written = write_pid_file(&path, &mut pidfile);
        // `flock` is tied to this descriptor. Keep it for the process lifetime
        // so a second daemon cannot lock the same pid file after `enter` returns.
        retain_pid_file(pidfile);
        written?;
    }

    Ok(())
}

/// Process-lifetime owner of the pid file. Dropping it would unlock the file.
static PID_FILE: Mutex<Option<File>> = Mutex::new(None);

fn retain_pid_file(file: File) {
    let mut slot = PID_FILE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *slot = Some(file);
}

fn open_pid_file(path: &Path) -> anyhow::Result<(PathBuf, File)> {
    let cwd = std::env::current_dir()
        .map_err(|e| anyhow!("failed to resolve pid file {}: {e}", path.display()))?;
    let path = resolve_pid_path(&cwd, path);
    let pidfile = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| anyhow!("failed to open pid file {}: {e}", path.display()))?;
    pidfile
        .lock()
        .map_err(|e| anyhow!("failed to lock pid file {}: {e}", path.display()))?;
    Ok((path, pidfile))
}

fn write_pid_file(path: &Path, pidfile: &mut File) -> anyhow::Result<()> {
    pidfile
        .set_len(0)
        .map_err(|e| anyhow!("failed to truncate pid file {}: {e}", path.display()))?;
    let pid = rustix::process::getpid();
    pidfile.write_fmt(format_args!("{pid}\n"))?;
    Ok(())
}

/// A relative pid path is joined to `cwd`, which must be the directory from
/// before `daemon(3)` changes it to `/`.
fn resolve_pid_path(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::{PID_FILE, resolve_pid_path, retain_pid_file};
    use std::fs::{File, OpenOptions, TryLockError};
    use std::path::Path;

    #[test]
    fn relative_pid_path_stays_in_the_original_directory() {
        let cwd = Path::new("/var/run");
        assert_eq!(
            resolve_pid_path(cwd, Path::new("vey.pid")),
            Path::new("/var/run/vey.pid")
        );
    }

    #[test]
    fn absolute_pid_path_is_unchanged() {
        let path = Path::new("/run/vey.pid");
        assert_eq!(resolve_pid_path(Path::new("/"), path), path);
    }

    #[test]
    fn pid_file_lock_is_held_after_the_local_file_is_gone() {
        let path = std::env::temp_dir().join(format!("vey-daemon-pid-{}", std::process::id()));
        let _cleanup = PidFileCleanup(&path);

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        file.lock().unwrap();
        retain_pid_file(file);

        let other = File::open(&path).unwrap();
        let err = other.try_lock().unwrap_err();
        assert!(matches!(err, TryLockError::WouldBlock));
    }

    struct PidFileCleanup<'a>(&'a Path);

    impl Drop for PidFileCleanup<'_> {
        fn drop(&mut self) {
            *PID_FILE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
            let _ = std::fs::remove_file(self.0);
        }
    }
}
