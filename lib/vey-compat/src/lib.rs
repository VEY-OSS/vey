/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod sched;
pub use sched::CpuAffinity;

mod hostname;
pub use hostname::hostname;

#[cfg(unix)]
mod runtime;
#[cfg(unix)]
pub use runtime::default_control_dir;
