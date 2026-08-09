/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::ffi::CString;
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use tokio::io::Interest;
use tokio::io::unix::AsyncFd;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use crate::ffi::{
    self, CPA_ACC_SVC_TYPE_CRYPTO, CPA_INST_RX_NOTIFY_BY_EVENT, CPA_STATUS_RETRY,
    CPA_STATUS_SUCCESS, CpaInstanceHandle, CpaStatus,
};
use crate::mem::virt_to_phys;

static USER_STARTED: OnceLock<CpaStatus> = OnceLock::new();

/// One QAT cryptographic instance with a poll task on a tokio runtime.
pub struct QatRuntime {
    instance: CpaInstanceHandle,
    stop: Arc<AtomicBool>,
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
        let start_sts =
            *USER_STARTED.get_or_init(|| unsafe { ffi::icp_sal_userStart(cname.as_ptr()) });
        if start_sts != CPA_STATUS_SUCCESS {
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

        let stop = Arc::new(AtomicBool::new(false));
        let stop_poll = Arc::clone(&stop);
        // Opaque handle is stable for the lifetime of this runtime.
        let instance_addr = instance as usize;
        let poll_task = handle.spawn(async move {
            poll_loop_epoll(instance_addr, fd, stop_poll).await;
        });

        Some(Arc::new(QatRuntime {
            instance,
            stop,
            poll_task: Mutex::new(Some(poll_task)),
        }))
    }

    pub(crate) fn instance(&self) -> CpaInstanceHandle {
        self.instance
    }
}

impl Drop for QatRuntime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.poll_task.lock()
            && let Some(task) = guard.take()
        {
            task.abort();
        }
        unsafe {
            let _ = ffi::icp_sal_CyPutFileDescriptor(self.instance);
            let _ = ffi::cpaCyStopInstance(self.instance);
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
        log::warn!(
            "qat instance {instance_id}: cpaInstanceSetResponseMode(EVENT) failed: {sts}"
        );
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

async fn poll_loop_epoll(instance_addr: usize, fd: RawFd, stop: Arc<AtomicBool>) {
    let async_fd = match AsyncFd::with_interest(fd, Interest::READABLE) {
        Ok(f) => f,
        Err(e) => {
            // Validated in `try_new`; should not happen.
            log::error!("qat AsyncFd::with_interest({fd}) failed: {e}");
            return;
        }
    };

    while !stop.load(Ordering::Relaxed) {
        let mut guard = match async_fd.readable().await {
            Ok(g) => g,
            Err(e) => {
                log::warn!("qat event FD readable wait failed: {e}");
                break;
            }
        };

        // Drain completions until the instance reports empty.
        loop {
            let sts =
                unsafe { ffi::icp_sal_CyPollInstance(instance_addr as CpaInstanceHandle, 0) };
            if sts == CPA_STATUS_RETRY {
                break;
            }
            if sts != CPA_STATUS_SUCCESS {
                log::debug!("icp_sal_CyPollInstance status {sts}");
                break;
            }
        }
        guard.clear_ready();
    }
}
