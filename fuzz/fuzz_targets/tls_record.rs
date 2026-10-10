/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#![no_main]

use libfuzzer_sys::fuzz_target;
use vey_codec::tls::{ClientHello, ExtensionType, HandshakeCoalescer, HandshakeMessage, Record};

fuzz_target!(|data: &[u8]| {
    let mut rest = data;
    let mut coalescer = HandshakeCoalescer::default();
    while !rest.is_empty() {
        let mut record = match Record::parse(rest) {
            Ok(record) => record,
            Err(_) => break,
        };
        let consumed = record.encoded_len();
        if consumed == 0 || consumed > rest.len() {
            break;
        }

        loop {
            if record.consume_done() {
                break;
            }
            match record.consume_handshake(&mut coalescer) {
                Ok(Some(message)) => observe_message(message),
                Ok(None) => {
                    if let Ok(Some(hello)) = coalescer.parse_client_hello() {
                        observe_hello(hello);
                    }
                }
                Err(_) => break,
            }
        }

        rest = &rest[consumed..];
    }
});

fn observe_message(message: HandshakeMessage<'_>) {
    if let Ok(hello) = message.parse_client_hello() {
        observe_hello(hello);
    }
}

fn observe_hello(hello: ClientHello<'_>) {
    std::hint::black_box(hello.cipher_suites.len());
    for ext_type in [
        ExtensionType::ServerName,
        ExtensionType::SupportedVersions,
        ExtensionType::KeyShare,
        ExtensionType::ApplicationLayerProtocolNegotiation,
        ExtensionType::SignatureAlgorithms,
    ] {
        if let Ok(value) = hello.get_ext(ext_type) {
            std::hint::black_box(value.map(|bytes| bytes.len()));
        }
    }
    for ext in hello.ext_iter() {
        if let Ok(ext) = ext {
            std::hint::black_box(ext.r#type());
            std::hint::black_box(ext.data().map(|bytes| bytes.len()));
        }
    }
}
