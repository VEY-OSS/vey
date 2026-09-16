/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::H2TaskContext;
use super::error::H2StreamTransferError;

mod task;
pub(super) use task::H2WebsocketTask;
