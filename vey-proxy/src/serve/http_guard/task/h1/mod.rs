/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::{CommonTaskContext, HttpGuardServerStats};

mod protocol;

mod forward;
mod pipeline;

use forward::HttpGuardForwardTask;
pub(crate) use pipeline::{
    HttpGuardPipelineReaderTask, HttpGuardPipelineStats, HttpGuardPipelineWriterTask,
};
