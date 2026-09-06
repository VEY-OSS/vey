/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::{CommonTaskContext, HttpGuardServerStats, protocol};

mod task;
pub(super) use task::HttpGuardForwardTask;

mod stats;
use stats::{
    HttpForwardTaskCltWrapperStats, HttpForwardTaskStats, HttpsForwardTaskCltWrapperStats,
};
