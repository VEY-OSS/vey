/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod buf;
pub use buf::SocketBufferConfig;

mod congestion;
pub use congestion::CongestionAlgorithm;

mod dns;
pub use dns::*;

mod domain;
pub use domain::{DomainName, DomainNameParseError};

mod egress;
pub use egress::{EgressArea, EgressInfo};

mod error;
pub use error::ConnectError;

mod haproxy;
pub use haproxy::{
    ProxyProtocolEncodeError, ProxyProtocolEncoder, ProxyProtocolV2Encoder, ProxyProtocolVersion,
};

mod host;
pub use host::Host;

mod pool;
pub use pool::ConnectionPoolConfig;

mod port;
pub use port::{PortRange, Ports};

mod proxy;
#[cfg(feature = "http")]
pub use proxy::HttpProxy;
pub use proxy::{Proxy, ProxyParseError, ProxyRequestType, Socks4Proxy, Socks5Proxy};

mod rate_limit;
pub use rate_limit::{
    RATE_LIMIT_SHIFT_MILLIS_DEFAULT, RATE_LIMIT_SHIFT_MILLIS_MAX, TcpSockSpeedLimitConfig,
    UdpSockSpeedLimitConfig,
};

mod socks;
pub use socks::SocksAuth;

mod tcp;
pub use tcp::*;

mod tls;
pub use tls::*;

mod udp;
pub use udp::{UdpConnectionTrackConfig, UdpListenConfig, UdpMiscSockOpts};

mod upstream;
pub use upstream::{UpstreamAddr, UpstreamHostRef, WeightedUpstreamAddr};

#[cfg(unix)]
mod interface;
#[cfg(unix)]
pub use interface::Interface;

#[cfg(feature = "http")]
mod http;
#[cfg(feature = "http")]
pub use self::http::*;

#[cfg(feature = "http")]
mod websocket;
#[cfg(feature = "http")]
pub use websocket::*;

#[cfg(feature = "rustls")]
mod rustls;
#[cfg(feature = "rustls")]
pub use self::rustls::*;

#[cfg(feature = "openssl")]
mod openssl;
#[cfg(feature = "openssl")]
pub use self::openssl::*;

#[cfg(feature = "quinn")]
mod quinn;
#[cfg(feature = "quinn")]
pub use self::quinn::*;
