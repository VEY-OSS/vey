/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod context;
pub(crate) use context::H2TaskContext;

mod error;
mod ping;
mod stream;

mod connection;
mod forward;
mod websocket;

pub(crate) use connection::HttpGuardH2ConnectionTask;
