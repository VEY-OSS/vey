/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::CommonTaskContext;

mod context;
pub(crate) use context::H2TaskContext;

mod error;
use error::H2StreamTransferError;

mod connection;
pub(crate) use connection::HttpGuardH2ConnectionTask;

mod forward;
use forward::H2ForwardTask;

mod stream;
use stream::H2StreamTask;

mod websocket;
use websocket::H2WebsocketTask;
