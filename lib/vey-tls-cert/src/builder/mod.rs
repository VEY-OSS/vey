/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod pkey;
mod serial;

mod key_usage;
pub use key_usage::KeyUsageBuilder;

mod subject;
pub use subject::SubjectNameBuilder;

mod time;
use time::{asn1_time_from_timestamp, checked_add_days, checked_sub_days, timestamp_now_utc_zoned};

mod server;
pub use server::{
    ServerCertBuilder, TlcpServerEncCertBuilder, TlcpServerSignCertBuilder, TlsServerCertBuilder,
};

mod client;
pub use client::{
    ClientCertBuilder, TlcpClientEncCertBuilder, TlcpClientSignCertBuilder, TlsClientCertBuilder,
};

mod root;
pub use root::RootCertBuilder;

mod intermediate;
pub use intermediate::IntermediateCertBuilder;

mod mimic;
pub use mimic::MimicCertBuilder;

#[cfg(test)]
mod tests {
    use openssl::asn1::Asn1TimeRef;
    use openssl::pkey::{PKey, Private};
    use openssl::x509::extension::{
        BasicConstraints, SubjectAlternativeName, SubjectKeyIdentifier,
    };
    use openssl::x509::{X509, X509Builder, X509NameBuilder};

    use super::*;
    use crate::ext::X509BuilderExt;

    fn asn1_days_from_now(days: i64) -> openssl::asn1::Asn1Time {
        let now = timestamp_now_utc_zoned();
        let timestamp = if days < 0 {
            checked_sub_days(&now, -days).unwrap()
        } else {
            checked_add_days(&now, days).unwrap()
        };
        asn1_time_from_timestamp(&timestamp).unwrap()
    }

    fn same_time(left: &Asn1TimeRef, right: &Asn1TimeRef) -> bool {
        let diff = left.diff(right).unwrap();
        diff.days == 0 && diff.secs == 0
    }

    fn self_signed_ca(
        not_before: &Asn1TimeRef,
        not_after: &Asn1TimeRef,
        path_len: Option<u32>,
    ) -> (X509, PKey<Private>) {
        let pkey = pkey::new_ec256().unwrap();
        let serial = serial::random_16().unwrap();
        let mut name = X509NameBuilder::new().unwrap();
        name.append_entry_by_text("CN", "test-ca").unwrap();
        let name = name.build();

        let mut builder = X509Builder::new().unwrap();
        builder.set_version(2).unwrap();
        builder.set_pubkey(&pkey).unwrap();
        builder.set_serial_number(&serial).unwrap();
        builder.set_not_before(not_before).unwrap();
        builder.set_not_after(not_after).unwrap();
        builder.set_subject_name(&name).unwrap();
        builder.set_issuer_name(&name).unwrap();

        let mut constraints = BasicConstraints::new();
        constraints.critical().ca();
        if let Some(len) = path_len {
            constraints.pathlen(len);
        }
        let constraints = constraints.build().unwrap();
        builder.append_extension(constraints).unwrap();

        let ctx = builder.x509v3_context(None, None);
        let ski = SubjectKeyIdentifier::new().build(&ctx).unwrap();
        builder.append_extension(ski).unwrap();
        builder.sign_with_optional_digest(&pkey, None).unwrap();
        (builder.build(), pkey)
    }

    #[test]
    fn intermediate_pathlen_is_capped_by_the_issuer() {
        let mut root = RootCertBuilder::new_ec256().unwrap();
        root.subject_builder_mut()
            .set_common_name("root".to_string());
        let root_cert = root.build(None).unwrap();

        let mut issuer = IntermediateCertBuilder::new_ec256().unwrap();
        issuer
            .subject_builder_mut()
            .set_common_name("issuer".to_string());
        let issuer_cert = issuer
            .build(Some(1), &root_cert, root.pkey(), None)
            .unwrap();
        assert_eq!(issuer_cert.pathlen(), Some(1));

        let mut child = IntermediateCertBuilder::new_ec256().unwrap();
        child
            .subject_builder_mut()
            .set_common_name("child".to_string());
        let child_cert = child
            .build(Some(5), &issuer_cert, issuer.pkey(), None)
            .unwrap();
        assert_eq!(child_cert.pathlen(), Some(0));

        let mut tighter = IntermediateCertBuilder::new_ec256().unwrap();
        tighter
            .subject_builder_mut()
            .set_common_name("tighter".to_string());
        let wide = issuer
            .build(Some(4), &root_cert, root.pkey(), None)
            .unwrap();
        let tighter_cert = tighter.build(Some(1), &wide, issuer.pkey(), None).unwrap();
        assert_eq!(tighter_cert.pathlen(), Some(1));
    }

    #[test]
    fn issued_not_before_is_not_earlier_than_the_issuer() {
        let not_before = asn1_days_from_now(30);
        let not_after = asn1_days_from_now(4000);
        let (ca_cert, ca_key) = self_signed_ca(&not_before, &not_after, None);

        let mut intermediate = IntermediateCertBuilder::new_ec256().unwrap();
        intermediate
            .subject_builder_mut()
            .set_common_name("intermediate".to_string());
        let intermediate_cert = intermediate
            .build(Some(0), &ca_cert, &ca_key, None)
            .unwrap();
        assert!(same_time(
            intermediate_cert.not_before(),
            ca_cert.not_before()
        ));

        let mut server_name = X509NameBuilder::new().unwrap();
        server_name
            .append_entry_by_text("CN", "server.example")
            .unwrap();
        let server_name = server_name.build();
        let mut server_san = SubjectAlternativeName::new();
        server_san.dns("server.example");
        let server = TlsServerCertBuilder::new_ec256().unwrap();
        let server_cert = server
            .build_with_subject(&server_name, server_san, &ca_cert, &ca_key, None)
            .unwrap();
        assert!(same_time(server_cert.not_before(), ca_cert.not_before()));

        let mut client_name = X509NameBuilder::new().unwrap();
        client_name
            .append_entry_by_text("CN", "client.example")
            .unwrap();
        let client_name = client_name.build();
        let mut client_san = SubjectAlternativeName::new();
        client_san.dns("client.example");
        let client = TlsClientCertBuilder::new_ec256().unwrap();
        let client_cert = client
            .build_with_subject(&client_name, client_san, &ca_cert, &ca_key, None)
            .unwrap();
        assert!(same_time(client_cert.not_before(), ca_cert.not_before()));
    }
}
