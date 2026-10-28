/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::ffi::CString;
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::Interest;
use tokio::io::unix::AsyncFd;
use tokio::runtime::Handle;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::ffi::{
    self, CPA_ACC_SVC_TYPE_CRYPTO, CPA_INST_RX_NOTIFY_BY_EVENT, CPA_STATUS_RETRY,
    CPA_STATUS_SUCCESS, CpaInstanceHandle, CpaStatus,
};
use crate::mem::virt_to_phys;

/// Remembers a successful `icp_sal_userStart` only. A failed start stays
/// retryable so a later `try_new` can succeed after qatmgr comes up.
struct UserStartGate {
    started: Mutex<bool>,
}

impl UserStartGate {
    const fn new() -> Self {
        Self {
            started: Mutex::new(false),
        }
    }

    fn ensure(&self, start: impl FnOnce() -> CpaStatus) -> Result<(), CpaStatus> {
        let mut started = self.started.lock().unwrap_or_else(|err| err.into_inner());
        if *started {
            return Ok(());
        }
        let status = start();
        if status == CPA_STATUS_SUCCESS {
            *started = true;
            Ok(())
        } else {
            Err(status)
        }
    }
}

static USER_START: UserStartGate = UserStartGate::new();

struct QatShared {
    instance: CpaInstanceHandle,
    stop: AtomicBool,
    inflight: AtomicUsize,
    shutdown: Notify,
}

unsafe impl Send for QatShared {}
unsafe impl Sync for QatShared {}

impl QatShared {
    fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
        self.shutdown.notify_one();
    }

    fn should_exit(&self) -> bool {
        self.stop.load(Ordering::Acquire) && self.inflight.load(Ordering::Acquire) == 0
    }

    fn finish_op(&self) {
        let prev = self.inflight.fetch_sub(1, Ordering::Release);
        debug_assert!(prev > 0);
        // The poll task may already be parked on the event FD. A failed submit
        // drops the in-flight count without a hardware completion, so wake it
        // again once the count hits zero.
        if prev == 1 && self.stop.load(Ordering::Acquire) {
            self.shutdown.notify_one();
        }
    }
}

impl Drop for QatShared {
    fn drop(&mut self) {
        if self.instance.is_null() {
            return;
        }
        unsafe {
            loop {
                let status = ffi::icp_sal_CyPollInstance(self.instance, 0);
                if status != CPA_STATUS_SUCCESS {
                    break;
                }
            }
            let _ = ffi::icp_sal_CyPutFileDescriptor(self.instance);
            let _ = ffi::cpaCyStopInstance(self.instance);
        }
    }
}

/// Decremented when the pending RSA/ECDSA buffer is freed, which is the callback
/// path that releases the DMA key material.
pub(crate) struct InFlight {
    shared: Arc<QatShared>,
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.shared.finish_op();
    }
}

/// One QAT cryptographic instance with a poll task on a tokio runtime.
pub struct QatRuntime {
    shared: Arc<QatShared>,
    poll_task: Mutex<Option<JoinHandle<()>>>,
}

unsafe impl Send for QatRuntime {}
unsafe impl Sync for QatRuntime {}

impl QatRuntime {
    /// Start `icp_sal_userStart` (once per process), claim `instance_id`, and
    /// spawn an epoll-driven poll loop on `handle`.
    ///
    /// Requires event notification (`CPA_INST_RX_NOTIFY_BY_EVENT` /
    /// `CyXIsPolled = 2`) and a valid `icp_sal_CyGetFileDescriptor`.
    ///
    /// Returns `None` when qatlib / device init fails or no event FD is
    /// available (caller should fall back).
    /// `instance_id` is the index into `cpaCyGetInstances` (0-based).
    pub fn try_new(process_name: &str, instance_id: u16, handle: &Handle) -> Option<Arc<Self>> {
        let cname = CString::new(process_name).ok()?;
        if let Err(start_sts) =
            USER_START.ensure(|| unsafe { ffi::icp_sal_userStart(cname.as_ptr()) })
        {
            log::warn!("icp_sal_userStart({process_name:?}) failed: {start_sts}");
            return None;
        }

        let mut num: u16 = 0;
        let sts = unsafe { ffi::cpaCyGetNumInstances(&mut num) };
        if sts != CPA_STATUS_SUCCESS || num == 0 {
            log::warn!("cpaCyGetNumInstances failed or zero instances (sts={sts}, n={num})");
            return None;
        }
        if instance_id >= num {
            log::warn!("qat instance_id {instance_id} out of range (available instances: {num})");
            return None;
        }

        let mut instances = vec![std::ptr::null_mut(); num as usize];
        let sts = unsafe { ffi::cpaCyGetInstances(num, instances.as_mut_ptr()) };
        if sts != CPA_STATUS_SUCCESS {
            log::warn!("cpaCyGetInstances failed: {sts}");
            return None;
        }
        let instance = instances[instance_id as usize];
        if instance.is_null() {
            log::warn!("qat instance_id {instance_id} handle is null");
            return None;
        }

        let sts = unsafe { ffi::cpaCySetAddressTranslation(instance, Some(virt_to_phys)) };
        if sts != CPA_STATUS_SUCCESS {
            log::warn!("cpaCySetAddressTranslation failed: {sts}");
            return None;
        }

        let sts = unsafe { ffi::cpaCyStartInstance(instance) };
        if sts != CPA_STATUS_SUCCESS {
            log::warn!("cpaCyStartInstance failed: {sts}");
            return None;
        }

        let Some(fd) = try_enable_event_fd(instance, instance_id) else {
            unsafe {
                let _ = ffi::cpaCyStopInstance(instance);
            }
            return None;
        };

        let shared = Arc::new(QatShared {
            instance,
            stop: AtomicBool::new(false),
            inflight: AtomicUsize::new(0),
            shutdown: Notify::new(),
        });
        let poll_shared = Arc::clone(&shared);
        let poll_task = handle.spawn(async move {
            let instance = poll_shared.instance as usize;
            poll_loop(poll_shared, fd, move || {
                drain_instance(instance as CpaInstanceHandle)
            })
            .await;
        });

        Some(Arc::new(QatRuntime {
            shared,
            poll_task: Mutex::new(Some(poll_task)),
        }))
    }

    pub(crate) fn instance(&self) -> CpaInstanceHandle {
        self.shared.instance
    }

    pub(crate) fn track_inflight(&self) -> InFlight {
        self.shared.inflight.fetch_add(1, Ordering::AcqRel);
        InFlight {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl Drop for QatRuntime {
    fn drop(&mut self) {
        // Detach the poll task instead of aborting it. Abort cancels the task
        // while it is parked on the event FD, so in-flight completions are
        // never polled and their DMA key buffers are never freed. The task
        // holds `QatShared` and stops the instance only after the count hits zero.
        release_poll_task(&self.shared, &self.poll_task);
    }
}

fn release_poll_task(shared: &QatShared, poll_task: &Mutex<Option<JoinHandle<()>>>) {
    shared.request_stop();
    match poll_task.lock() {
        Ok(mut guard) => {
            guard.take();
        }
        Err(poisoned) => {
            poisoned.into_inner().take();
        }
    }
}

/// Request event notification and obtain a pollable FD.
fn try_enable_event_fd(instance: CpaInstanceHandle, instance_id: u16) -> Option<RawFd> {
    let sts = unsafe {
        ffi::cpaInstanceSetResponseMode(
            instance,
            CPA_ACC_SVC_TYPE_CRYPTO,
            CPA_INST_RX_NOTIFY_BY_EVENT,
        )
    };
    if sts != CPA_STATUS_SUCCESS {
        log::warn!("qat instance {instance_id}: cpaInstanceSetResponseMode(EVENT) failed: {sts}");
        return None;
    }

    let mut fd: libc::c_int = -1;
    let sts = unsafe { ffi::icp_sal_CyGetFileDescriptor(instance, &mut fd) };
    if sts != CPA_STATUS_SUCCESS || fd < 0 {
        log::warn!(
            "qat instance {instance_id}: icp_sal_CyGetFileDescriptor failed (sts={sts}, fd={fd})"
        );
        return None;
    }

    // Match QAT Engine: non-blocking FD for epoll readiness.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags >= 0 {
        let _ = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    }

    log::info!("qat instance {instance_id}: epoll poll via FD {fd}");
    Some(fd)
}

fn drain_instance(instance: CpaInstanceHandle) {
    loop {
        let status = unsafe { ffi::icp_sal_CyPollInstance(instance, 0) };
        if status == CPA_STATUS_RETRY {
            break;
        }
        if status != CPA_STATUS_SUCCESS {
            log::debug!("icp_sal_CyPollInstance status {status}");
            break;
        }
    }
}

async fn poll_loop(shared: Arc<QatShared>, fd: RawFd, mut drain: impl FnMut() + Send) {
    let async_fd = match unsafe { AsyncFd::register_with_interest(fd, Interest::READABLE) } {
        Ok(fd) => fd,
        Err(e) => {
            // Validated in `try_new`; should not happen.
            log::error!("qat AsyncFd::with_interest({fd}) failed: {e}");
            return;
        }
    };

    loop {
        if shared.should_exit() {
            break;
        }
        tokio::select! {
            biased;

            _ = shared.shutdown.notified() => {
                drain();
            }
            ready = async_fd.readable() => {
                match ready {
                    Ok(mut guard) => {
                        drain();
                        guard.clear_ready();
                    }
                    Err(e) => {
                        log::warn!("qat event FD readable wait failed: {e}");
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::task::{Context, Poll, Waker};
    use std::time::Duration;

    use super::*;

    #[test]
    fn user_start_failure_is_not_sticky() {
        let gate = UserStartGate::new();
        let calls = AtomicUsize::new(0);
        let fail = || {
            calls.fetch_add(1, Ordering::Relaxed);
            -1
        };
        assert!(gate.ensure(fail).is_err());
        assert!(gate.ensure(fail).is_err());
        assert!(
            gate.ensure(|| {
                calls.fetch_add(1, Ordering::Relaxed);
                CPA_STATUS_SUCCESS
            })
            .is_ok()
        );
        assert!(
            gate.ensure(|| {
                calls.fetch_add(1, Ordering::Relaxed);
                panic!("started gate must not call start again");
            })
            .is_ok()
        );
        assert_eq!(calls.load(Ordering::Relaxed), 3);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn shutdown_wakes_poll_and_drains_inflight() {
        let mut fds = [0; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        for fd in fds {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            assert!(flags >= 0);
            assert_eq!(
                unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) },
                0
            );
        }
        let read_fd = fds[0];
        let write_fd = fds[1];

        let shared = Arc::new(QatShared {
            instance: std::ptr::null_mut(),
            stop: AtomicBool::new(false),
            inflight: AtomicUsize::new(1),
            shutdown: Notify::new(),
        });
        let finished = Arc::new(AtomicBool::new(false));
        let task_shared = Arc::clone(&shared);
        let task_finished = Arc::clone(&finished);
        let handle = tokio::spawn(async move {
            let drain_shared = Arc::clone(&task_shared);
            poll_loop(task_shared, read_fd, move || drain_shared.finish_op()).await;
            task_finished.store(true, Ordering::Release);
        });
        let slot = Mutex::new(Some(handle));

        tokio::task::yield_now().await;
        release_poll_task(&shared, &slot);

        let done = tokio::time::timeout(Duration::from_secs(1), async {
            while !finished.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await;
        unsafe {
            libc::close(read_fd);
            libc::close(write_fd);
        }
        assert!(done.is_ok(), "poll task was cancelled before it drained");
        assert!(finished.load(Ordering::Acquire));
        assert!(shared.should_exit());
        assert_eq!(shared.inflight.load(Ordering::Acquire), 0);
    }

    #[test]
    fn inflight_drop_wakes_a_stopped_session() {
        let shared = Arc::new(QatShared {
            instance: std::ptr::null_mut(),
            stop: AtomicBool::new(true),
            inflight: AtomicUsize::new(0),
            shutdown: Notify::new(),
        });
        shared.inflight.fetch_add(1, Ordering::AcqRel);
        let ticket = InFlight {
            shared: Arc::clone(&shared),
        };
        drop(ticket);

        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        assert!(matches!(
            std::pin::pin!(shared.shutdown.notified()).poll(&mut cx),
            Poll::Ready(())
        ));
        assert!(shared.should_exit());
    }
}
