/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::{CommonTaskContext, HttpProxyServerStats, protocol};

mod stats;
use stats::{MasqueUdpTaskCltWrapperStats, MasqueUdpTaskServerCltWrapperStats};

mod recv;
use recv::MasqueUdpRecv;

mod send;
use send::MasqueUdpSend;

mod task;
pub(super) use task::HttpProxyMasqueUdpTask;
