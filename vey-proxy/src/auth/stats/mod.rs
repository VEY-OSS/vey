/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::sync::Arc;

use smallvec::SmallVec;

mod forbidden;
pub(crate) use forbidden::{UserForbiddenSnapshot, UserForbiddenStats};

mod request;
pub(crate) use request::{UserRequestAliveGuard, UserRequestSnapshot, UserRequestStats};

mod traffic;
pub(crate) use traffic::{
    UserTrafficSnapshot, UserTrafficStats, UserUpstreamTrafficSnapshot, UserUpstreamTrafficStats,
};

mod site;
pub(crate) use site::UserSiteStats;

mod duration;
pub(crate) use duration::{UserSiteDurationRecorder, UserSiteDurationStats};

pub(crate) type UserRequestStatsList = SmallVec<[Arc<UserRequestStats>; 4]>;
pub(crate) type UserTrafficStatsList = SmallVec<[Arc<UserTrafficStats>; 4]>;
pub(crate) type UserUpstreamTrafficStatsList = SmallVec<[Arc<UserUpstreamTrafficStats>; 4]>;
