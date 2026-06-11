/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::{CommonTaskContext, HttpProxyServerStats, protocol};

mod stats;
use stats::{HttpConnectUdpTaskCltWrapperStats, HttpConnectUdpTaskServerCltWrapperStats};

mod recv;
use recv::HttpConnectUdpRecv;

mod send;
use send::HttpConnectUdpSend;

mod task;
pub(super) use task::HttpProxyConnectUdpTask;
