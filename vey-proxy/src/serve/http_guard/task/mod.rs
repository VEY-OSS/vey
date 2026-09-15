/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::HttpGuardServerStats;
use crate::config::server::http_guard::HttpGuardServerConfig;

mod common;
pub(super) use common::CommonTaskContext;

mod h1;
pub(super) use h1::{
    H1TaskContext, HttpGuardPipelineReaderTask, HttpGuardPipelineStats, HttpGuardPipelineWriterTask,
};

mod h2;
pub(super) use h2::{H2TaskContext, HttpGuardH2ConnectionTask};
