/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::ptr;

use crate::ffi::{self, CpaFlatBuffer};

/// Contiguous DMA buffer allocated via USDM (`qaeMemAllocNUMA`).
pub struct DmaBuf {
    ptr: *mut u8,
    len: usize,
}

unsafe impl Send for DmaBuf {}
unsafe impl Sync for DmaBuf {}

impl DmaBuf {
    pub fn alloc(len: usize) -> Option<Self> {
        if len == 0 {
            return None;
        }
        let ptr = unsafe { ffi::qaeMemAllocNUMA(len, 0, 64) as *mut u8 };
        if ptr.is_null() {
            return None;
        }
        unsafe {
            ptr::write_bytes(ptr, 0, len);
        }
        Some(DmaBuf { ptr, len })
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    pub fn copy_from_be_padded(&mut self, src: &[u8]) -> bool {
        if src.len() > self.len {
            return false;
        }
        let len = self.len;
        let dst = self.as_mut_slice();
        dst.fill(0);
        dst[len - src.len()..].copy_from_slice(src);
        true
    }

    pub fn flat(&mut self) -> CpaFlatBuffer {
        CpaFlatBuffer {
            dataLenInBytes: self.len as u32,
            pData: self.ptr,
        }
    }
}

impl Drop for DmaBuf {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            let mut p = self.ptr as *mut libc::c_void;
            unsafe {
                ffi::qaeMemFreeNUMA(&mut p);
            }
            self.ptr = ptr::null_mut();
        }
    }
}

pub unsafe extern "C" fn virt_to_phys(virt: *mut libc::c_void) -> ffi::CpaPhysicalAddr {
    unsafe { ffi::qaeVirtToPhysNUMA(virt) }
}
