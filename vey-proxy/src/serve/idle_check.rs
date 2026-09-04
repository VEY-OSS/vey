/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::sync::Arc;

use vey_io_ext::{IdleCheck, IdleForceQuitReason, IdleInterval, IdleWheel};

use super::ServerQuitPolicy;
use crate::auth::User;

pub(crate) struct ServerIdleChecker {
    pub(crate) idle_wheel: Arc<IdleWheel>,
    pub(crate) user: Option<Arc<User>>,
    pub(crate) tenant: Option<Arc<User>>,
    pub(crate) max_idle_count: usize,
    pub(crate) server_quit_policy: Arc<ServerQuitPolicy>,
}

impl ServerIdleChecker {
    pub(crate) fn new(
        idle_wheel: Arc<IdleWheel>,
        user: Option<Arc<User>>,
        tenant: Option<Arc<User>>,
        max_idle_count: usize,
        server_quit_policy: Arc<ServerQuitPolicy>,
    ) -> Self {
        ServerIdleChecker {
            idle_wheel,
            user,
            tenant,
            max_idle_count,
            server_quit_policy,
        }
    }
}

impl IdleCheck for ServerIdleChecker {
    fn interval_timer(&self) -> IdleInterval {
        self.idle_wheel.register()
    }

    fn check_quit(&self, idle_count: usize) -> bool {
        idle_count > self.max_idle_count
    }

    fn check_force_quit(&self) -> Option<IdleForceQuitReason> {
        if self.user.as_ref().is_some_and(|u| u.is_blocked())
            || self.tenant.as_ref().is_some_and(|t| t.is_blocked())
        {
            return Some(IdleForceQuitReason::UserBlocked);
        }

        if self.server_quit_policy.force_quit() {
            return Some(IdleForceQuitReason::ServerQuit);
        }

        None
    }
}
