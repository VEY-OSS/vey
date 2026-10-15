/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::io;

#[derive(Clone, Default)]
pub struct CpuAffinityImpl {}

impl CpuAffinityImpl {
    pub const fn max_cpu_id(&self) -> usize {
        // Affinity is unsupported, so every ID reaches add_id's Unsupported error.
        usize::MAX
    }

    pub fn add_id(&mut self, _id: usize) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "cpu affinity is not supported on this system",
        ))
    }

    pub fn apply_to_local_thread(&self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "cpu affinity is not supported on this system",
        ))
    }
}
