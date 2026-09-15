/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use http::{Uri, Version};

pub(crate) struct WebSocketTaskNotes {
    pub(crate) version: Version,
    pub(crate) uri: Uri,
    pub(crate) uri_log_max_chars: usize,
    pub(crate) rsp_status: u16,
    pub(crate) origin_status: u16,
}

impl WebSocketTaskNotes {
    pub(crate) fn new(version: Version, uri: Uri, uri_log_max_chars: usize) -> Self {
        WebSocketTaskNotes {
            version,
            uri,
            uri_log_max_chars,
            rsp_status: 0,
            origin_status: 0,
        }
    }
}
