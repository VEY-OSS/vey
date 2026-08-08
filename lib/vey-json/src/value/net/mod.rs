/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod base;
#[cfg(feature = "acl-rule")]
pub use base::as_ip_network;
pub use base::{
    as_congestion_algorithm, as_domain, as_egress_area, as_host, as_ipaddr, as_upstream_addr,
};

mod ports;
pub use ports::{as_port_range, as_ports};

mod proxy;
pub use proxy::as_proxy_request_type;

mod tcp;
pub use tcp::{as_tcp_connect_config, as_tcp_keepalive_config, as_tcp_misc_sock_opts};

mod tls;
pub use tls::as_tls_version;

mod udp;
pub use udp::as_udp_misc_sock_opts;

#[cfg(feature = "http")]
mod http;
#[cfg(feature = "http")]
pub use http::as_http_keepalive_config;
