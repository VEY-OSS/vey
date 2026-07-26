/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::{Context, anyhow};
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;
use rmpv::ValueRef;

use vey_types::net::{Host, TlsCertUsage, TlsServiceType};

use super::{CacheQueryKey, FakeCertPair, response_key, response_key_id};

pub(super) struct Response {
    host: Host,
    service: TlsServiceType,
    usage: TlsCertUsage,
    certs: Vec<X509>,
    key: Option<PKey<Private>>,
    ttl: u32,
}

impl Response {
    fn new(protective_ttl: u32) -> Self {
        Response {
            host: Host::empty(),
            service: TlsServiceType::Http,
            usage: TlsCertUsage::TlsServer,
            certs: Vec::new(),
            key: None,
            ttl: protective_ttl,
        }
    }

    fn set(&mut self, k: ValueRef, v: ValueRef) -> anyhow::Result<()> {
        match k {
            ValueRef::String(s) => {
                let key = s
                    .as_str()
                    .ok_or_else(|| anyhow!("invalid string key {k}"))?;
                match vey_msgpack::key::normalize(key).as_str() {
                    response_key::HOST => {
                        self.host = vey_msgpack::value::as_host(&v)
                            .context(format!("invalid host string value for key {key}"))?;
                    }
                    response_key::SERVICE => {
                        self.service = vey_msgpack::value::as_tls_service_type(&v)
                            .context(format!("invalid tls service type value for key {key}"))?;
                    }
                    response_key::USAGE => {
                        self.usage = vey_msgpack::value::as_tls_cert_usage(&v)
                            .context(format!("invalid tls cert usage value for key {key}"))?;
                    }
                    response_key::CERT_CHAIN => {
                        self.certs = vey_msgpack::value::as_openssl_certificates(&v)
                            .context(format!("invalid tls certificate value for key {key}"))?;
                    }
                    response_key::PRIVATE_KEY => {
                        let key = vey_msgpack::value::as_openssl_private_key(&v)
                            .context(format!("invalid tls private key value for key {key}"))?;
                        self.key = Some(key);
                    }
                    response_key::TTL => {
                        self.ttl = vey_msgpack::value::as_u32(&v)
                            .context(format!("invalid u32 value for key {key}"))?;
                    }
                    _ => {} // ignore unknown keys
                }
            }
            ValueRef::Integer(i) => {
                let key_id = i.as_u64().ok_or_else(|| anyhow!("invalid u64 key {k}"))?;
                match key_id {
                    response_key_id::HOST => {
                        self.host = vey_msgpack::value::as_host(&v)
                            .context(format!("invalid host string value for key id {key_id}"))?;
                    }
                    response_key_id::SERVICE => {
                        self.service = vey_msgpack::value::as_tls_service_type(&v).context(
                            format!("invalid tls service type value for key id {key_id}"),
                        )?;
                    }
                    response_key_id::USAGE => {
                        self.usage = vey_msgpack::value::as_tls_cert_usage(&v)
                            .context(format!("invalid tls cert usage value for key id {key_id}"))?;
                    }
                    response_key_id::CERT_CHAIN => {
                        self.certs = vey_msgpack::value::as_openssl_certificates(&v).context(
                            format!("invalid tls certificate value for key id {key_id}"),
                        )?;
                    }
                    response_key_id::PRIVATE_KEY => {
                        let key = vey_msgpack::value::as_openssl_private_key(&v).context(
                            format!("invalid tls private key value for key id {key_id}"),
                        )?;
                        self.key = Some(key);
                    }
                    response_key_id::TTL => {
                        self.ttl = vey_msgpack::value::as_u32(&v)
                            .context(format!("invalid u32 value for key id {key_id}"))?;
                    }
                    _ => {} // ignore unknown keys
                }
            }
            _ => return Err(anyhow!("unsupported key type: {k}")),
        }
        Ok(())
    }

    pub(super) fn parse(v: ValueRef, protective_ttl: u32) -> anyhow::Result<Self> {
        if let ValueRef::Map(map) = v {
            let mut response = Response::new(protective_ttl);
            for (k, v) in map {
                response.set(k, v)?;
            }
            Ok(response)
        } else {
            Err(anyhow!("the response data type should be 'map'"))
        }
    }

    pub(super) fn into_parts(self) -> anyhow::Result<(CacheQueryKey, FakeCertPair, u32)> {
        if self.certs.is_empty() {
            return Err(anyhow!("no cert chain set"));
        }
        let key = self.key.ok_or_else(|| anyhow!("no private key set"))?;
        Ok((
            CacheQueryKey::new(self.service, self.usage, self.host),
            FakeCertPair {
                certs: self.certs,
                key,
            },
            self.ttl,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rmpv::ValueRef;

    use vey_types::net::{Host, TlsCertUsage, TlsServiceType};

    use super::super::{response_key, response_key_id};
    use super::Response;
    use crate::test_util;

    fn encode_map(entries: Vec<(ValueRef<'_>, ValueRef<'_>)>) -> Vec<u8> {
        let mut buf = Vec::new();
        rmpv::encode::write_value_ref(&mut buf, &ValueRef::Map(entries)).unwrap();
        buf
    }

    #[test]
    fn parse_response_string_keys() {
        let (_, _, pem, der_key) = test_util::self_signed_cert_key();
        let buf = encode_map(vec![
            (
                ValueRef::String(response_key::HOST.into()),
                ValueRef::String("s.example".into()),
            ),
            (
                ValueRef::String(response_key::SERVICE.into()),
                ValueRef::Integer((TlsServiceType::Http as u8).into()),
            ),
            (
                ValueRef::String(response_key::USAGE.into()),
                ValueRef::Integer((TlsCertUsage::TlsServer as u8).into()),
            ),
            (
                ValueRef::String(response_key::CERT_CHAIN.into()),
                ValueRef::String(pem.as_str().into()),
            ),
            (
                ValueRef::String(response_key::PRIVATE_KEY.into()),
                ValueRef::Binary(&der_key),
            ),
            (
                ValueRef::String(response_key::TTL.into()),
                ValueRef::Integer(42u32.into()),
            ),
            (
                ValueRef::String("ignored".into()),
                ValueRef::String("x".into()),
            ),
        ]);
        let mut data = buf.as_slice();
        let v = rmpv::decode::read_value_ref(&mut data).unwrap();
        let rsp = Response::parse(v, 10).unwrap();
        let (key, pair, ttl) = rsp.into_parts().unwrap();
        assert_eq!(ttl, 42);
        assert_eq!(key.index.host, Host::from_str("s.example").unwrap());
        assert_eq!(pair.certs.len(), 1);
    }

    #[test]
    fn parse_response_integer_keys_and_errors() {
        let (_, _, pem, der_key) = test_util::self_signed_cert_key();
        let buf = encode_map(vec![
            (
                ValueRef::Integer(response_key_id::HOST.into()),
                ValueRef::String("i.example".into()),
            ),
            (
                ValueRef::Integer(response_key_id::SERVICE.into()),
                ValueRef::Integer((TlsServiceType::Http as u8).into()),
            ),
            (
                ValueRef::Integer(response_key_id::USAGE.into()),
                ValueRef::Integer((TlsCertUsage::TlsServer as u8).into()),
            ),
            (
                ValueRef::Integer(response_key_id::CERT_CHAIN.into()),
                ValueRef::String(pem.as_str().into()),
            ),
            (
                ValueRef::Integer(response_key_id::PRIVATE_KEY.into()),
                ValueRef::Binary(&der_key),
            ),
            (
                ValueRef::Integer(response_key_id::TTL.into()),
                ValueRef::Integer(7u32.into()),
            ),
        ]);
        let mut data = buf.as_slice();
        let v = rmpv::decode::read_value_ref(&mut data).unwrap();
        let (_, _, ttl) = Response::parse(v, 10).unwrap().into_parts().unwrap();
        assert_eq!(ttl, 7);

        // non-map
        let mut buf = Vec::new();
        rmpv::encode::write_value_ref(&mut buf, &ValueRef::String("x".into())).unwrap();
        let mut data = buf.as_slice();
        let v = rmpv::decode::read_value_ref(&mut data).unwrap();
        assert!(Response::parse(v, 10).is_err());

        // missing cert / key
        let empty = Response::new(10);
        assert!(empty.into_parts().is_err());
    }
}
