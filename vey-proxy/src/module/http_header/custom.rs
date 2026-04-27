/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::cell::RefCell;
use std::io::Write;
use std::net::{IpAddr, SocketAddr};

use base64::prelude::*;
use chrono::{DateTime, Utc};
use http::HeaderName;

use vey_types::net::{EgressInfo, HttpHeaderMap, HttpHeaderValue, HttpServerId};

// chained final info header
const UPSTREAM_ID: &str = "X-VEY-Upstream-ID";
const UPSTREAM_ADDR: &str = "X-VEY-Upstream-Addr";
const OUTGOING_IP: &str = "X-VEY-Outgoing-IP";

// local info header (append)
const REMOTE_CONNECTION_INFO: &str = "X-VEY-Remote-Connection-Info";
const DYNAMIC_EGRESS_INFO: &str = "X-VEY-Dynamic-Egress-Info";

thread_local! {
    static TL_BUF: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(256));
}

fn set_value_for_remote_connection_info(
    v: &mut Vec<u8>,
    server_id: &HttpServerId,
    bind: Option<IpAddr>,
    local: Option<SocketAddr>,
    remote: Option<SocketAddr>,
    expire: &Option<DateTime<Utc>>,
) {
    v.extend_from_slice(server_id.as_bytes());
    if let Some(ip) = bind {
        let _ = write!(v, "; bind={ip}");
    }
    if let Some(addr) = local {
        let _ = write!(v, "; local={addr}");
    }
    if let Some(addr) = remote {
        let _ = write!(v, "; remote={addr}");
    }
    if let Some(expire) = expire {
        let _ = write!(
            v,
            "; expire={}",
            expire.format_with_items(vey_datetime::format::std::RFC3339_FIXED_MICROSECOND.iter())
        );
    }
}

pub(crate) fn remote_connection_info(
    server_id: &HttpServerId,
    bind: Option<IpAddr>,
    local: Option<SocketAddr>,
    remote: Option<SocketAddr>,
    expire: &Option<DateTime<Utc>>,
) -> String {
    let mut buf = Vec::<u8>::with_capacity(256);
    buf.extend_from_slice(REMOTE_CONNECTION_INFO.as_bytes());
    buf.extend_from_slice(b": ");
    set_value_for_remote_connection_info(&mut buf, server_id, bind, local, remote, expire);
    buf.extend_from_slice(b"\r\n");
    // we can make sure that the vec contains only UTF-8 chars
    unsafe { String::from_utf8_unchecked(buf) }
}

pub(crate) fn set_remote_connection_info(
    headers: &mut HttpHeaderMap,
    server_id: &HttpServerId,
    bind: Option<IpAddr>,
    local: Option<SocketAddr>,
    remote: Option<SocketAddr>,
    expire: &Option<DateTime<Utc>>,
) {
    TL_BUF.with_borrow_mut(|buf| {
        set_value_for_remote_connection_info(buf, server_id, bind, local, remote, expire);
        let mut value = unsafe { HttpHeaderValue::from_buf_unchecked(buf.clone()) };
        value.set_original_name(REMOTE_CONNECTION_INFO);
        headers.append(HeaderName::from_static(REMOTE_CONNECTION_INFO), value);
        buf.clear();
    })
}

fn set_value_for_dynamic_egress_info(
    v: &mut Vec<u8>,
    server_id: &HttpServerId,
    egress: &EgressInfo,
) {
    v.extend_from_slice(server_id.as_bytes());
    if let Some(isp) = &egress.isp() {
        let _ = write!(v, "; isp={}", BASE64_STANDARD.encode(isp));
    }
    if let Some(ip) = &egress.ip() {
        let _ = write!(v, "; ip={ip}");
    }
    if let Some(area) = &egress.area() {
        let _ = write!(v, "; area={}", BASE64_STANDARD.encode(area.to_string()));
    }
}

pub(crate) fn dynamic_egress_info(server_id: &HttpServerId, egress: &EgressInfo) -> String {
    let mut buf = Vec::<u8>::with_capacity(256);
    buf.extend_from_slice(DYNAMIC_EGRESS_INFO.as_bytes());
    buf.extend_from_slice(b": ");
    set_value_for_dynamic_egress_info(&mut buf, server_id, egress);
    buf.extend_from_slice(b"\r\n");
    // we can make sure that the vec contains only UTF-8 chars
    unsafe { String::from_utf8_unchecked(buf) }
}

pub(crate) fn set_dynamic_egress_info(
    headers: &mut HttpHeaderMap,
    server_id: &HttpServerId,
    egress: &EgressInfo,
) {
    TL_BUF.with_borrow_mut(|buf| {
        set_value_for_dynamic_egress_info(buf, server_id, egress);
        let mut value = unsafe { HttpHeaderValue::from_buf_unchecked(buf.clone()) };
        value.set_original_name(DYNAMIC_EGRESS_INFO);
        headers.append(HeaderName::from_static(DYNAMIC_EGRESS_INFO), value);
        buf.clear()
    })
}

pub(crate) fn set_upstream_id(headers: &mut HttpHeaderMap, id: &HttpServerId) {
    if !headers.contains_key(HeaderName::from_static(UPSTREAM_ID)) {
        let mut value = id.to_header_value();
        value.set_original_name(UPSTREAM_ID);
        headers.append(HeaderName::from_static(UPSTREAM_ID), value);
    }
}

pub(crate) fn upstream_addr(addr: SocketAddr) -> String {
    // header name should sync with UPSTREAM_ADDR
    format!("{UPSTREAM_ADDR}: {addr}\r\n")
}

pub(crate) fn set_upstream_addr(headers: &mut HttpHeaderMap, addr: SocketAddr) {
    if !headers.contains_key(HeaderName::from_static(UPSTREAM_ADDR)) {
        let mut value = unsafe { HttpHeaderValue::from_string_unchecked(addr.to_string()) };
        value.set_original_name(UPSTREAM_ADDR);
        headers.append(HeaderName::from_static(UPSTREAM_ADDR), value);
    }
}

pub(crate) fn outgoing_ip(ip: IpAddr) -> String {
    // header name should sync with OUTGOING_IP
    format!("{OUTGOING_IP}: {ip}\r\n")
}

pub(crate) fn set_outgoing_ip(headers: &mut HttpHeaderMap, addr: SocketAddr) {
    if !headers.contains_key(HeaderName::from_static(OUTGOING_IP)) {
        let mut value = unsafe { HttpHeaderValue::from_string_unchecked(addr.ip().to_string()) };
        value.set_original_name(OUTGOING_IP);
        headers.append(HeaderName::from_static(OUTGOING_IP), value);
    }
}
