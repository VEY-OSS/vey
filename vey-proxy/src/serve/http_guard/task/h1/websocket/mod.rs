/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::{CommonTaskContext, protocol};

mod stats;

mod task;
pub(super) use task::HttpGuardWebsocketTask;
