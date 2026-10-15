/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::mem;

#[derive(Clone)]
pub struct CpuAffinityImpl {
    cpu_set: libc::cpu_set_t,
}

impl Default for CpuAffinityImpl {
    fn default() -> Self {
        CpuAffinityImpl {
            cpu_set: unsafe { mem::zeroed() },
        }
    }
}

pub const fn max_cpu_id() -> usize {
    let bytes = size_of::<libc::cpu_set_t>();
    (bytes << 3) - 1
}

impl CpuAffinityImpl {
    pub const fn max_cpu_id(&self) -> usize {
        max_cpu_id()
    }

    pub(super) fn add_id(&mut self, id: usize) -> io::Result<()> {
        unsafe {
            libc::CPU_SET(id, &mut self.cpu_set);
        }
        Ok(())
    }

    /// `sched_setaffinity(0)` changes every thread in the process.
    /// This applies the mask to the calling thread only.
    pub(super) fn apply_to_local_thread(&self) -> io::Result<()> {
        let r = unsafe {
            libc::pthread_setaffinity_np(
                libc::pthread_self(),
                size_of::<libc::cpu_set_t>() as libc::size_t,
                &self.cpu_set,
            )
        };
        if r != 0 {
            Err(io::Error::from_raw_os_error(r))
        } else {
            Ok(())
        }
    }
}
