/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod recv;
pub(crate) use recv::HttpConnectUdpRecvBuffer;

mod send;
pub(crate) use send::HttpConnectUdpSendBuffer;
