/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use vey_io_ext::{ArcLimitedReaderStats, LimitedReaderStats};

use super::HttpExposeServerStats;
use crate::auth::UserTrafficStats;

pub(super) struct UntrustedCltReadWrapperStats {
    server: Arc<HttpExposeServerStats>,
    site_io: Arc<UserTrafficStats>,
}

impl UntrustedCltReadWrapperStats {
    pub(super) fn new_obj(
        server: &Arc<HttpExposeServerStats>,
        site_io: Arc<UserTrafficStats>,
    ) -> ArcLimitedReaderStats {
        let stats = UntrustedCltReadWrapperStats {
            server: Arc::clone(server),
            site_io,
        };
        Arc::new(stats)
    }
}

impl LimitedReaderStats for UntrustedCltReadWrapperStats {
    fn add_read_bytes(&self, size: usize) {
        let size = size as u64;
        self.server.io_untrusted.add_in_bytes(size);
        self.site_io.io.http_forward.add_in_bytes(size);
    }
}
