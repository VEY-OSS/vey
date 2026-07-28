/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod base;
#[cfg(feature = "acl-rule")]
pub use base::as_ip_network;
pub use base::{
    as_congestion_algorithm, as_domain, as_env_sockaddr, as_host, as_ipaddr, as_ipv4addr,
    as_ipv6addr, as_sockaddr, as_upstream_addr, as_url, as_weighted_sockaddr,
    as_weighted_upstream_addr,
};

mod buf;
pub use buf::as_socket_buffer_config;

mod haproxy;
pub use haproxy::as_proxy_protocol_version;

mod pool;
pub use pool::as_connection_pool_config;

mod port;
pub use port::{as_port_range, as_ports};

mod proxy;
pub use proxy::as_proxy_request_type;

mod tcp;
pub use tcp::{
    as_happy_eyeballs_config, as_tcp_connect_config, as_tcp_keepalive_config, as_tcp_listen_config,
    as_tcp_misc_sock_opts,
};

mod tls;
pub use tls::as_tls_version;

mod udp;
pub use udp::{as_udp_conn_track_config, as_udp_listen_config, as_udp_misc_sock_opts};

#[cfg(unix)]
mod interface;
#[cfg(unix)]
pub use interface::as_interface;

#[cfg(feature = "http")]
mod http;
#[cfg(feature = "http")]
pub use self::http::{
    as_http_forward_capability, as_http_forwarded_header_type, as_http_header_name,
    as_http_header_value_string, as_http_keepalive_config, as_http_path_and_query,
    as_http_server_id,
};

#[cfg(feature = "rustls")]
mod dns;
#[cfg(feature = "rustls")]
pub use dns::as_dns_encryption_protocol_builder;
