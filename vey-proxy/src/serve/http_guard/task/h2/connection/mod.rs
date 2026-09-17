/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::{H2StreamTask, H2TaskContext};

mod stats;
use stats::{H2ConcurrencyStats, H2ConnectionCltWrapperStats, H2ConnectionTaskStats};

mod task;
pub(crate) use task::HttpGuardH2ConnectionTask;
