/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#![no_main]

use libfuzzer_sys::fuzz_target;
use vey_types::net::{
    AcceptTransferEncodingValue, AuthorizationValueParser, ConnectionValue, DomainName, Host,
    TransferEncodingValue,
};

fuzz_target!(|data: &[u8]| {
    let mut transfer = TransferEncodingValue::default();
    if transfer.parse(data).is_ok() {
        std::hint::black_box((
            transfer.chunked(),
            transfer.compress_kind(),
            transfer.body_compressed(),
        ));
    }

    let mut accept = AcceptTransferEncodingValue::default();
    if accept.parse(data).is_ok() {
        std::hint::black_box(accept.trailers());
    }

    let mut connection = ConnectionValue::default();
    connection.parse(data);
    connection.parse_keep_alive(b"keep-alive", data);
    std::hint::black_box((
        connection.upgrade(),
        connection.extra_headers().len(),
        connection.keep_alive_header().is_empty(),
    ));

    if let Some(auth) = AuthorizationValueParser::parse(data) {
        std::hint::black_box((auth.scheme(), auth.content().len(), auth.is_session_based()));
    }

    if let Ok(domain) = DomainName::parse(data) {
        std::hint::black_box(domain.as_fqdn_str().len());
    }
    if let Some(host) = Host::parse_smtp_host_address(data) {
        std::hint::black_box(host.to_string());
    }
});
