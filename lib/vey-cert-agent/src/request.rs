/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::{Context, anyhow};
use openssl::x509::X509;
use rmpv::ValueRef;

use vey_types::net::{Host, TlsCertUsage, TlsServiceType};

use super::{request_key, request_key_id, response_key_id};

pub struct Request {
    pub(crate) host: Host,
    service: TlsServiceType,
    usage: TlsCertUsage,
    pub(crate) cert: Option<X509>,
}

impl Default for Request {
    fn default() -> Self {
        Request {
            host: Host::empty(),
            service: TlsServiceType::Http,
            usage: TlsCertUsage::TlsServer,
            cert: None,
        }
    }
}

impl Request {
    #[inline]
    pub fn host(&self) -> &Host {
        &self.host
    }

    #[inline]
    pub fn cert(&self) -> Option<&X509> {
        self.cert.as_ref()
    }

    #[inline]
    pub fn cert_usage(&self) -> TlsCertUsage {
        self.usage
    }

    fn check(&self) -> anyhow::Result<()> {
        if self.host.is_empty() {
            return Err(anyhow!("no host value set"));
        }
        Ok(())
    }

    fn set(&mut self, k: ValueRef, v: ValueRef) -> anyhow::Result<()> {
        match k {
            ValueRef::String(s) => {
                let key = s
                    .as_str()
                    .ok_or_else(|| anyhow!("invalid string key {k}"))?;
                match vey_msgpack::key::normalize(key).as_str() {
                    request_key::HOST => self
                        .set_host_value(v)
                        .context(format!("invalid string value for key {key}")),
                    request_key::SERVICE => {
                        self.service = vey_msgpack::value::as_tls_service_type(&v)
                            .context(format!("invalid tls service type value for key {key}"))?;
                        Ok(())
                    }
                    request_key::USAGE => {
                        self.usage = vey_msgpack::value::as_tls_cert_usage(&v)
                            .context(format!("invalid tls cert usage value for key {key}"))?;
                        Ok(())
                    }
                    request_key::CERT => {
                        let cert = vey_msgpack::value::as_openssl_certificate(&v)
                            .context(format!("invalid mimic cert value for key {key}"))?;
                        self.cert = Some(cert);
                        Ok(())
                    }
                    _ => Err(anyhow!("invalid key {key}")),
                }
            }
            ValueRef::Integer(i) => {
                let key_id = i.as_u64().ok_or_else(|| anyhow!("invalid u64 key {k}"))?;
                match key_id {
                    request_key_id::HOST => self
                        .set_host_value(v)
                        .context(format!("invalid host string value for key id {key_id}")),
                    request_key_id::SERVICE => {
                        self.service = vey_msgpack::value::as_tls_service_type(&v).context(
                            format!("invalid tls service type value for key id {key_id}"),
                        )?;
                        Ok(())
                    }
                    request_key_id::USAGE => {
                        self.usage = vey_msgpack::value::as_tls_cert_usage(&v)
                            .context(format!("invalid tls cert usage value for key id {key_id}"))?;
                        Ok(())
                    }
                    request_key_id::CERT => {
                        let cert = vey_msgpack::value::as_openssl_certificate(&v)
                            .context(format!("invalid mimic cert value for key id {key_id}"))?;
                        self.cert = Some(cert);
                        Ok(())
                    }
                    _ => Err(anyhow!("invalid key id {key_id}")),
                }
            }
            _ => Err(anyhow!("unsupported key type: {k}")),
        }
    }

    fn set_host_value(&mut self, v: ValueRef) -> anyhow::Result<()> {
        self.host = vey_msgpack::value::as_host(&v)?;
        Ok(())
    }

    pub fn parse_req(mut data: &[u8]) -> anyhow::Result<Self> {
        let v = rmpv::decode::read_value_ref(&mut data)
            .map_err(|e| anyhow!("invalid req data: {e}"))?;

        let mut request = Request::default();
        if let ValueRef::Map(map) = v {
            for (k, v) in map {
                request.set(k, v)?;
            }
        } else {
            request
                .set_host_value(v)
                .context("invalid single host string value")?;
        }

        request.check()?;
        Ok(request)
    }

    pub fn encode_rsp(&self, pem_cert: &str, der_key: &[u8], ttl: u32) -> anyhow::Result<Vec<u8>> {
        let host_str = self.host().to_string();
        let map = vec![
            (
                ValueRef::Integer(response_key_id::HOST.into()),
                ValueRef::String(host_str.as_str().into()),
            ),
            (
                ValueRef::Integer(response_key_id::SERVICE.into()),
                ValueRef::Integer((self.service as u8).into()),
            ),
            (
                ValueRef::Integer(response_key_id::USAGE.into()),
                ValueRef::Integer((self.usage as u8).into()),
            ),
            (
                ValueRef::Integer(response_key_id::CERT_CHAIN.into()),
                ValueRef::String(pem_cert.into()),
            ),
            (
                ValueRef::Integer(response_key_id::PRIVATE_KEY.into()),
                ValueRef::Binary(der_key),
            ),
            (
                ValueRef::Integer(response_key_id::TTL.into()),
                ValueRef::Integer(ttl.into()),
            ),
        ];
        let mut buf = Vec::with_capacity(4096);
        let v = ValueRef::Map(map);
        rmpv::encode::write_value_ref(&mut buf, &v)
            .map_err(|e| anyhow!("msgpack encode failed: {e}"))?;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rmpv::ValueRef;

    use vey_types::net::{Host, TlsCertUsage, TlsServiceType};

    use super::super::{request_key, request_key_id};
    use super::Request;
    use crate::test_util;

    fn encode_map(entries: Vec<(ValueRef<'_>, ValueRef<'_>)>) -> Vec<u8> {
        let mut buf = Vec::new();
        rmpv::encode::write_value_ref(&mut buf, &ValueRef::Map(entries)).unwrap();
        buf
    }

    #[test]
    fn parse_req_host_only_string() {
        let mut buf = Vec::new();
        rmpv::encode::write_value_ref(&mut buf, &ValueRef::String("only.example".into())).unwrap();
        let req = Request::parse_req(&buf).unwrap();
        assert_eq!(req.host(), &Host::from_str("only.example").unwrap());
        assert_eq!(req.cert_usage(), TlsCertUsage::TlsServer);
        assert!(req.cert().is_none());
    }

    #[test]
    fn parse_req_string_keys_with_mimic_cert() {
        let (cert, _, _, _) = test_util::self_signed_cert_key();
        let der = cert.to_der().unwrap();
        let buf = encode_map(vec![
            (
                ValueRef::String(request_key::HOST.into()),
                ValueRef::String("str.example".into()),
            ),
            (
                ValueRef::String(request_key::SERVICE.into()),
                ValueRef::Integer((TlsServiceType::Http as u8).into()),
            ),
            (
                ValueRef::String(request_key::USAGE.into()),
                ValueRef::Integer((TlsCertUsage::TLsServerTongsuo as u8).into()),
            ),
            (
                ValueRef::String(request_key::CERT.into()),
                ValueRef::Binary(&der),
            ),
        ]);
        let req = Request::parse_req(&buf).unwrap();
        assert_eq!(req.host(), &Host::from_str("str.example").unwrap());
        assert_eq!(req.cert_usage(), TlsCertUsage::TLsServerTongsuo);
        assert!(req.cert().is_some());
    }

    #[test]
    fn parse_req_integer_key_ids() {
        let buf = encode_map(vec![
            (
                ValueRef::Integer(request_key_id::HOST.into()),
                ValueRef::String("id.example".into()),
            ),
            (
                ValueRef::Integer(request_key_id::SERVICE.into()),
                ValueRef::Integer((TlsServiceType::Smtp as u8).into()),
            ),
            (
                ValueRef::Integer(request_key_id::USAGE.into()),
                ValueRef::Integer((TlsCertUsage::TlsServer as u8).into()),
            ),
        ]);
        let req = Request::parse_req(&buf).unwrap();
        assert_eq!(req.host(), &Host::from_str("id.example").unwrap());
        assert_eq!(req.cert_usage(), TlsCertUsage::TlsServer);
    }

    #[test]
    fn parse_req_rejects_empty_host_and_bad_input() {
        let buf = encode_map(vec![(
            ValueRef::Integer(request_key_id::SERVICE.into()),
            ValueRef::Integer((TlsServiceType::Http as u8).into()),
        )]);
        assert!(Request::parse_req(&buf).is_err());
        assert!(Request::parse_req(b"not-msgpack").is_err());

        let buf = encode_map(vec![(
            ValueRef::String("unknown_key".into()),
            ValueRef::String("x".into()),
        )]);
        assert!(Request::parse_req(&buf).is_err());
    }

    #[test]
    fn encode_rsp_roundtrips_through_response() {
        use crate::response::Response;

        let req = Request {
            host: Host::from_str("rsp.example").unwrap(),
            service: TlsServiceType::Http,
            usage: TlsCertUsage::TlsServer,
            cert: None,
        };
        let (_, _, pem, der_key) = test_util::self_signed_cert_key();
        let buf = req.encode_rsp(&pem, &der_key, 120).unwrap();

        let mut data = buf.as_slice();
        let v = rmpv::decode::read_value_ref(&mut data).unwrap();
        let rsp = Response::parse(v, 10).unwrap();
        let (key, pair, ttl) = rsp.into_parts().unwrap();
        assert_eq!(ttl, 120);
        assert_eq!(key.index.host, Host::from_str("rsp.example").unwrap());
        assert_eq!(pair.certs.len(), 1);
    }
}
