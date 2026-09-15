/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::ops::Deref;
use std::sync::Arc;

use super::super::CommonTaskContext;
use crate::serve::http_guard::HttpHost;
use crate::site::SiteContext;

#[derive(Clone)]
pub(crate) struct H1TaskContext {
    pub(crate) common: CommonTaskContext,
    pub(crate) pinned_host: Option<Arc<HttpHost>>,
    pub(crate) site_ctx: Option<SiteContext>,
}

impl Deref for H1TaskContext {
    type Target = CommonTaskContext;

    fn deref(&self) -> &Self::Target {
        &self.common
    }
}
