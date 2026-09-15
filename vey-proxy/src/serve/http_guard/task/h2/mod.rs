/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod context;
pub(crate) use context::H2TaskContext;

mod connection;
mod error;
mod forward;
mod ping;
mod stats;
mod stream;
mod websocket;

pub(crate) use connection::HttpGuardH2ConnectionTask;
