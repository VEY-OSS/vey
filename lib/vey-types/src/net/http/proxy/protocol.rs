/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpProxySubProtocol {
    TcpConnect,
    UdpConnect,
    HttpForward,
    HttpsForward,
    FtpOverHttp,
}
