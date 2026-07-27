/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::hash::{Hash, Hasher};

use anyhow::anyhow;
use openssl::pkey::{PKey, Private};
use openssl::ssl::SslRef;
use openssl::x509::X509;

use vey_types::net::{Host, TlsCertUsage, TlsServiceType};

mod protocol;
pub use protocol::*;

mod response;
use response::Response;

mod request;
pub use request::Request;

mod query;
use query::QueryRuntime;

mod config;
pub use config::CertAgentConfig;

mod handle;
pub use handle::CertAgentHandle;

mod runtime;
pub use runtime::*;

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
struct CacheIndexKey {
    service: TlsServiceType,
    usage: TlsCertUsage,
    host: Host,
}

#[derive(Clone, Debug)]
struct CacheQueryKey {
    index: CacheIndexKey,
    mimic_cert: Option<X509>,
}

impl CacheQueryKey {
    fn new(service: TlsServiceType, usage: TlsCertUsage, host: Host) -> Self {
        CacheQueryKey {
            index: CacheIndexKey {
                service,
                usage,
                host,
            },
            mimic_cert: None,
        }
    }

    fn set_mimic_cert(&mut self, cert: X509) {
        self.mimic_cert = Some(cert);
    }

    fn encode(&self) -> Result<Vec<u8>, rmpv::encode::Error> {
        use rmpv::ValueRef;

        let mut map = Vec::with_capacity(4);
        let host = self.index.host.to_string();
        map.push((
            ValueRef::Integer(request_key_id::HOST.into()),
            ValueRef::String(host.as_str().into()),
        ));
        map.push((
            ValueRef::Integer(request_key_id::SERVICE.into()),
            ValueRef::Integer((self.index.service as u8).into()),
        ));
        map.push((
            ValueRef::Integer(request_key_id::USAGE.into()),
            ValueRef::Integer((self.index.usage as u8).into()),
        ));
        if let Some(cert) = &self.mimic_cert
            && let Ok(der) = cert.to_der()
        {
            map.push((
                ValueRef::Integer(request_key_id::CERT.into()),
                ValueRef::Binary(&der),
            ));
            let mut buf = Vec::with_capacity(320 + der.len());
            rmpv::encode::write_value_ref(&mut buf, &ValueRef::Map(map))?;
            return Ok(buf);
        };
        let mut buf = Vec::with_capacity(320);
        rmpv::encode::write_value_ref(&mut buf, &ValueRef::Map(map))?;
        Ok(buf)
    }
}

impl Hash for CacheQueryKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

impl PartialEq for CacheQueryKey {
    fn eq(&self, other: &Self) -> bool {
        self.index.eq(&other.index)
    }
}

impl Eq for CacheQueryKey {}

#[derive(Clone)]
pub struct FakeCertPair {
    certs: Vec<X509>,
    key: PKey<Private>,
}

impl FakeCertPair {
    pub fn add_to_ssl(self, ssl: &mut SslRef) -> anyhow::Result<()> {
        let FakeCertPair { certs, key } = self;
        let mut certs_iter = certs.into_iter();
        let Some(leaf_cert) = certs_iter.next() else {
            return Err(anyhow!("no certificate found"));
        };
        ssl.set_certificate(&leaf_cert)
            .map_err(|e| anyhow!("failed to set certificate: {e}"))?;
        for cert in certs_iter {
            ssl.add_chain_cert(cert)
                .map_err(|e| anyhow!("failed to add chain cert: {e}"))?;
        }
        ssl.set_private_key(&key)
            .map_err(|e| anyhow!("failed to set private key: {e}"))?;
        Ok(())
    }

    #[cfg(feature = "tongsuo")]
    pub fn add_enc_to_tlcp(self, ssl: &mut SslRef) -> anyhow::Result<()> {
        let FakeCertPair { certs, key } = self;
        let mut certs_iter = certs.into_iter();
        let Some(leaf_cert) = certs_iter.next() else {
            return Err(anyhow!("no certificate found"));
        };
        ssl.set_enc_certificate(&leaf_cert)
            .map_err(|e| anyhow!("failed to set enc certificate: {e}"))?;
        for cert in certs_iter {
            ssl.add_chain_cert(cert)
                .map_err(|e| anyhow!("failed to add chain cert: {e}"))?;
        }
        ssl.set_enc_private_key(&key)
            .map_err(|e| anyhow!("failed to set enc private key: {e}"))?;
        Ok(())
    }

    #[cfg(feature = "tongsuo")]
    pub fn add_sign_to_tlcp(self, ssl: &mut SslRef) -> anyhow::Result<()> {
        let FakeCertPair { certs, key } = self;
        let mut certs_iter = certs.into_iter();
        let Some(leaf_cert) = certs_iter.next() else {
            return Err(anyhow!("no certificate found"));
        };
        ssl.set_sign_certificate(&leaf_cert)
            .map_err(|e| anyhow!("failed to set sign certificate: {e}"))?;
        for cert in certs_iter {
            ssl.add_chain_cert(cert)
                .map_err(|e| anyhow!("failed to add chain cert: {e}"))?;
        }
        ssl.set_sign_private_key(&key)
            .map_err(|e| anyhow!("failed to set sign private key: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod test_util {
    use openssl::asn1::Asn1Time;
    use openssl::bn::BigNum;
    use openssl::hash::MessageDigest;
    use openssl::nid::Nid;
    use openssl::pkey::{PKey, Private};
    use openssl::rsa::Rsa;
    use openssl::x509::{X509, X509NameBuilder};

    /// Self-signed RSA leaf used by unit tests.
    pub(crate) fn self_signed_cert_key() -> (X509, PKey<Private>, String, Vec<u8>) {
        let rsa = Rsa::generate(2048).unwrap();
        let pkey = PKey::from_rsa(rsa).unwrap();

        let mut name = X509NameBuilder::new().unwrap();
        name.append_entry_by_nid(Nid::COMMONNAME, "test.example")
            .unwrap();
        let name = name.build();

        let mut builder = X509::builder().unwrap();
        builder.set_version(2).unwrap();
        let serial = BigNum::from_u32(1).unwrap().to_asn1_integer().unwrap();
        builder.set_serial_number(&serial).unwrap();
        builder.set_subject_name(&name).unwrap();
        builder.set_issuer_name(&name).unwrap();
        builder.set_pubkey(&pkey).unwrap();
        builder
            .set_not_before(&Asn1Time::days_from_now(0).unwrap())
            .unwrap();
        builder
            .set_not_after(&Asn1Time::days_from_now(365).unwrap())
            .unwrap();
        builder.sign(&pkey, MessageDigest::sha256()).unwrap();
        let cert = builder.build();

        let pem = String::from_utf8(cert.to_pem().unwrap()).unwrap();
        let der_key = pkey.private_key_to_der().unwrap();
        (cert, pkey, pem, der_key)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::str::FromStr;

    use openssl::ssl::{Ssl, SslContext, SslMethod};

    use vey_types::net::{Host, TlsCertUsage, TlsServiceType};

    use super::{CacheQueryKey, FakeCertPair, test_util};

    fn hash_of(key: &CacheQueryKey) -> u64 {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn cache_query_key_eq_and_hash_ignore_mimic_cert() {
        let host = Host::from_str("a.example").unwrap();
        let mut a = CacheQueryKey::new(TlsServiceType::Http, TlsCertUsage::TlsServer, host.clone());
        let b = CacheQueryKey::new(TlsServiceType::Http, TlsCertUsage::TlsServer, host);
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));

        let (cert, _, _, _) = test_util::self_signed_cert_key();
        a.set_mimic_cert(cert);
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn cache_query_key_encode_with_and_without_mimic() {
        let host = Host::from_str("encode.example").unwrap();
        let key = CacheQueryKey::new(TlsServiceType::Http, TlsCertUsage::TlsServer, host.clone());
        let buf = key.encode().unwrap();
        assert!(!buf.is_empty());

        let mut with_mimic =
            CacheQueryKey::new(TlsServiceType::Http, TlsCertUsage::TlsServer, host);
        let (cert, _, _, _) = test_util::self_signed_cert_key();
        with_mimic.set_mimic_cert(cert);
        let buf_mimic = with_mimic.encode().unwrap();
        assert!(buf_mimic.len() > buf.len());
    }

    #[test]
    fn fake_cert_pair_add_to_ssl() {
        let (cert, key, _, _) = test_util::self_signed_cert_key();
        let pair = FakeCertPair {
            certs: vec![cert],
            key,
        };
        let ctx = SslContext::builder(SslMethod::tls_server())
            .unwrap()
            .build();
        let mut ssl = Ssl::new(&ctx).unwrap();
        pair.add_to_ssl(&mut ssl).unwrap();

        let empty = FakeCertPair {
            certs: Vec::new(),
            key: test_util::self_signed_cert_key().1,
        };
        let mut ssl = Ssl::new(&ctx).unwrap();
        assert!(empty.add_to_ssl(&mut ssl).is_err());
    }
}
