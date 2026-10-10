/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#![no_main]

use std::io::Cursor;

use libfuzzer_sys::fuzz_target;
use tokio::io::BufReader;
use vey_socks::v4a::SocksV4aRequest;
use vey_socks::v5::{Socks5Request, UdpInput};

fuzz_target!(|data: &[u8]| {
    if let Ok((header_len, addr)) = UdpInput::parse_header(data) {
        std::hint::black_box((header_len, addr.port(), addr.host_str().len()));
    }

    with_runtime(|runtime| {
        runtime.block_on(async {
            let mut reader = BufReader::new(Cursor::new(data));
            if let Ok(request) = Socks5Request::recv(&mut reader).await {
                let peer = request.udp_peer_addr().ok();
                std::hint::black_box((
                    request.command,
                    request.upstream.port(),
                    request.upstream.host_str().len(),
                    peer,
                ));
            }

            let mut reader = BufReader::new(Cursor::new(data));
            if let Ok(request) = SocksV4aRequest::recv(&mut reader).await {
                std::hint::black_box((
                    request.command,
                    request.upstream.port(),
                    request.upstream.host_str().len(),
                    request.user_id.len(),
                ));
            }
        });
    });
});

fn with_runtime(f: impl FnOnce(&tokio::runtime::Runtime)) {
    thread_local! {
        static RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("current-thread runtime");
    }
    RUNTIME.with(f);
}
