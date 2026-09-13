/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::CommonTaskContext;

mod connection;
mod error;
mod forward;
mod origin;
mod ping;
mod stats;
mod stream;
mod websocket;

pub(crate) use connection::HttpGuardH2ConnectionTask;
