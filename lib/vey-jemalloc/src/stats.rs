/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use core::ffi::CStr;
use core::marker::PhantomData;
use core::ptr;

const MIB_MAX_LENGTH: usize = 8;

/// Return  the  total number of bytes in active pages collected in an unsynchronized manner,
/// without requiring an epoch update
pub fn approximate_active() -> Option<JemallocStatsEntry<usize>> {
    JemallocStatsEntry::new(c"approximate_stats.active")
}

/// Total number of bytes allocated by the application.
pub fn allocated() -> Option<JemallocStatsEntry<usize>> {
    JemallocStatsEntry::new(c"stats.allocated")
}

/// Total number of bytes in active pages allocated by the application.
pub fn active() -> Option<JemallocStatsEntry<usize>> {
    JemallocStatsEntry::new(c"stats.active")
}

/// Total number of bytes dedicated to metadata
pub fn metadata() -> Option<JemallocStatsEntry<usize>> {
    JemallocStatsEntry::new(c"stats.metadata")
}

/// Number of transparent huge pages (THP) used for metadata.
pub fn metadata_thp() -> Option<JemallocStatsEntry<usize>> {
    JemallocStatsEntry::new(c"stats.metadata_thp")
}

/// Maximum  number of bytes in physically resident data pages mapped by the allocator, comprising
/// all pages dedicated to allocator metadata, pages backing active allocations, and unused dirty
/// pages.
pub fn resident() -> Option<JemallocStatsEntry<usize>> {
    JemallocStatsEntry::new(c"stats.resident")
}

/// Total number of bytes in active extents mapped by the allocator.
pub fn mapped() -> Option<JemallocStatsEntry<usize>> {
    JemallocStatsEntry::new(c"stats.mapped")
}

/// Total  number of bytes in virtual memory mappings that were retained rather than being returned
/// to the operating system via e.g.  munmap(2) or similar.
pub fn retained() -> Option<JemallocStatsEntry<usize>> {
    JemallocStatsEntry::new(c"stats.retained")
}

/// Number  of  times  that the realloc() was called with a non-NULL pointer argument and a 0 size
/// argument.
pub fn zero_reallocs() -> Option<JemallocStatsEntry<usize>> {
    JemallocStatsEntry::new(c"stats.zero_reallocs")
}

/// Number of background threads running currently.
pub fn background_thread_num_threads() -> Option<JemallocStatsEntry<usize>> {
    JemallocStatsEntry::new(c"stats.background_thread.num_threads")
}

/// Total number of runs from all background threads.
pub fn background_thread_num_runs() -> Option<JemallocStatsEntry<u64>> {
    JemallocStatsEntry::new(c"stats.background_thread.num_runs")
}

/// Average run interval in nanoseconds of background threads.
pub fn background_thread_run_interval() -> Option<JemallocStatsEntry<u64>> {
    JemallocStatsEntry::new(c"stats.background_thread.run_interval")
}

pub struct JemallocStatsEntry<T> {
    name: &'static CStr,
    mib: [usize; MIB_MAX_LENGTH],
    mib_len: usize,
    _phantom: PhantomData<T>,
}

impl<T> JemallocStatsEntry<T>
where
    T: Copy + Default,
{
    pub fn new(name: &'static CStr) -> Option<Self> {
        let mut mib = [0usize; MIB_MAX_LENGTH];
        let mut mib_len = MIB_MAX_LENGTH;
        unsafe {
            let ret = jemalloc_sys::mallctlnametomib(
                name.as_ptr(),
                mib.as_mut_ptr(),
                ptr::from_mut(&mut mib_len),
            );
            if ret != 0 {
                return None;
            }
        }

        Some(JemallocStatsEntry {
            name,
            mib,
            mib_len,
            _phantom: PhantomData,
        })
    }

    pub fn name(&self) -> &CStr {
        self.name
    }

    pub fn value(&self) -> Option<T> {
        let mut value_len = size_of::<T>();
        let mut value = T::default();

        unsafe {
            let ret = jemalloc_sys::mallctlbymib(
                self.mib.as_ptr(),
                self.mib_len,
                ptr::from_mut(&mut value) as *mut _,
                ptr::from_mut(&mut value_len),
                ptr::null_mut(),
                0,
            );
            if ret != 0 {
                return None;
            }
        }

        Some(value)
    }
}
