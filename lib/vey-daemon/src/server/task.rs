/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::OnceLock;

use jiff::Timestamp;
use uuid::{ContextV1, Timestamp as UuidTimestamp, Uuid};

static UUID_CONTEXT: OnceLock<ContextV1> = OnceLock::new();
static UUID_NODE_ID: OnceLock<[u8; 6]> = OnceLock::new();

pub fn generate_uuid(time: &Timestamp) -> Uuid {
    let context = UUID_CONTEXT.get_or_init(|| ContextV1::new(rand::random()));
    let node_id = UUID_NODE_ID.get_or_init(|| {
        let mut bytes = [0u8; 6];
        rand::fill(&mut bytes);
        bytes
    });

    let ts = UuidTimestamp::from_unix(
        context,
        time.as_second() as u64,
        // jiff has no leap seconds; subsec is already in 0..=999_999_999 for now().
        time.subsec_nanosecond() as u32,
    );
    Uuid::new_v1(ts, node_id)
}
