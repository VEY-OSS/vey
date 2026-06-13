/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod size;
pub use size::{as_u16, as_u32, as_u64, as_usize};

mod time;
pub use time::as_duration;
