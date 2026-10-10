/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#![no_main]

use libfuzzer_sys::fuzz_target;
use vey_codec::ldap::{LdapMessage, LdapResult, LdapSequence};

fuzz_target!(|data: &[u8]| {
    if let Ok(message) = LdapMessage::parse(data, 1 << 20) {
        std::hint::black_box(message.id());
        let payload = message.payload();
        observe_result(payload);
        observe_sequences(payload);
        std::hint::black_box(message.encoded_size());
    }
    observe_result(data);
    observe_sequences(data);
});

fn observe_result(data: &[u8]) {
    if let Ok(result) = LdapResult::parse(data) {
        std::hint::black_box((
            result.result_code(),
            result.is_success(),
            result.matched_dn().len(),
            result.diagnostic_message().len(),
            result.encoded_len(),
        ));
    }
}

fn observe_sequences(data: &[u8]) {
    if let Ok(seq) = LdapSequence::parse_octet_string(data) {
        std::hint::black_box((seq.data().len(), seq.encoded_len()));
    }
    if let Ok(seq) = LdapSequence::parse_referrals_sequence(data) {
        std::hint::black_box((seq.data().len(), seq.encoded_len()));
    }
    if let Ok(seq) = LdapSequence::parse_bind_response(data) {
        std::hint::black_box((seq.data().len(), seq.encoded_len()));
    }
    if let Ok(seq) = LdapSequence::parse_extended_response(data) {
        std::hint::black_box((seq.data().len(), seq.encoded_len()));
    }
    if let Ok(seq) = LdapSequence::parse_extended_response_oid(data) {
        std::hint::black_box((seq.data().len(), seq.encoded_len()));
    }
}
