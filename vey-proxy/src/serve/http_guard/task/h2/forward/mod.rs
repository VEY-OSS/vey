/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::{
    H2StreamTransferError, H2TaskContext, OriginConnection, OriginH1Sender, OriginH2Sender,
};

mod h1;
mod task;
pub(super) use task::H2ForwardTask;
