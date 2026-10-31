/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use vey_statsd_client::{StatsdClient, StatsdTagGroup};

use super::ServerMetricExt;
use crate::listen::{ListenSnapshot, ListenStats};

const METRIC_NAME_LISTEN_INSTANCE_COUNT: &str = "listen.instance.count";
const METRIC_NAME_LISTEN_ACCEPTED: &str = "listen.accepted";
const METRIC_NAME_LISTEN_DROPPED: &str = "listen.dropped";
const METRIC_NAME_LISTEN_TIMEOUT: &str = "listen.timeout";
const METRIC_NAME_LISTEN_FAILED: &str = "listen.failed";
const METRIC_NAME_LISTEN_DATAGRAM_RECV_TOTAL: &str = "listen.datagram.recv.total";
const METRIC_NAME_LISTEN_DATAGRAM_RECV_DROPPED: &str = "listen.datagram.recv.dropped";
const METRIC_NAME_LISTEN_DATAGRAM_RECV_OK: &str = "listen.datagram.recv.ok";
const METRIC_NAME_LISTEN_DATAGRAM_SEND_QUEUED: &str = "listen.datagram.send.queued";
const METRIC_NAME_LISTEN_DATAGRAM_SEND_FAILED: &str = "listen.datagram.send.failed";
const METRIC_NAME_LISTEN_DATAGRAM_SEND_OK: &str = "listen.datagram.send.ok";

pub fn emit_listen_stats(
    client: &mut StatsdClient,
    stats: &Arc<ListenStats>,
    snap: &mut ListenSnapshot,
) {
    let mut common_tags = StatsdTagGroup::default();
    common_tags.add_server_tags(stats.name(), stats.is_running(), stats.stat_id());

    client
        .gauge_with_tags(
            METRIC_NAME_LISTEN_INSTANCE_COUNT,
            stats.running_runtime_count(),
            &common_tags,
        )
        .send();

    macro_rules! emit_field {
        ($field:ident, $name:expr) => {
            let new_value = stats.$field();
            if new_value != 0 || snap.$field != 0 {
                let diff_value = new_value.wrapping_sub(snap.$field);
                client
                    .count_with_tags($name, diff_value, &common_tags)
                    .send();
                snap.$field = new_value;
            }
        };
    }

    emit_field!(accepted, METRIC_NAME_LISTEN_ACCEPTED);
    emit_field!(dropped, METRIC_NAME_LISTEN_DROPPED);
    emit_field!(timeout, METRIC_NAME_LISTEN_TIMEOUT);
    emit_field!(failed, METRIC_NAME_LISTEN_FAILED);
    emit_field!(datagram_recv_total, METRIC_NAME_LISTEN_DATAGRAM_RECV_TOTAL);
    emit_field!(
        datagram_recv_dropped,
        METRIC_NAME_LISTEN_DATAGRAM_RECV_DROPPED
    );
    emit_field!(datagram_recv_ok, METRIC_NAME_LISTEN_DATAGRAM_RECV_OK);
    emit_field!(
        datagram_send_queued,
        METRIC_NAME_LISTEN_DATAGRAM_SEND_QUEUED
    );
    emit_field!(
        datagram_send_failed,
        METRIC_NAME_LISTEN_DATAGRAM_SEND_FAILED
    );
    emit_field!(datagram_send_ok, METRIC_NAME_LISTEN_DATAGRAM_SEND_OK);
}
