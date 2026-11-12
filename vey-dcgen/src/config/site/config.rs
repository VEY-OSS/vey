/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, anyhow};
use openssl::asn1::{Asn1Time, Asn1TimeRef};
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;
use saphyr::Yaml;

use vey_types::metrics::NodeName;
use vey_yaml::YamlDocPosition;

/// Parsed site. `position` is set when the map was loaded from a file.
#[derive(Clone)]
pub(crate) struct SiteConfig {
    id: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) tls: Option<Arc<SiteKeyCert>>,
    pub(crate) tlcp_sign: Option<Arc<SiteKeyCert>>,
    pub(crate) tlcp_enc: Option<Arc<SiteKeyCert>>,
}

/// One certificate pair returned by the protocol. The chain is PEM and the
/// private key is DER.
pub(crate) struct SiteKeyCert {
    pub(crate) certificate: String,
    pub(crate) private_key: Vec<u8>,
    not_after_unix: i64,
    ttl: u32,
}

impl SiteKeyCert {
    pub(crate) fn certificate(&self) -> &str {
        &self.certificate
    }

    pub(crate) fn private_key(&self) -> &[u8] {
        &self.private_key
    }

    pub(crate) fn ttl(&self) -> anyhow::Result<u32> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        if self.not_after_unix <= now {
            return Err(anyhow!("the predefined certificate is already expired"));
        }
        let seconds = self.not_after_unix - now;
        let seconds = u32::try_from(seconds).unwrap_or(u32::MAX);
        Ok(self.ttl.min(seconds))
    }

    fn new(
        certs: Option<Vec<X509>>,
        key: Option<PKey<Private>>,
        ttl: u32,
        label: &str,
    ) -> anyhow::Result<Option<Self>> {
        match (certs, key) {
            (None, None) => Ok(None),
            (Some(certs), Some(key)) => Ok(Some(Self::from_validated(certs, key, ttl)?)),
            _ => Err(anyhow!(
                "{label} certificate and private key must both be set"
            )),
        }
    }

    fn from_validated(certs: Vec<X509>, key: PKey<Private>, ttl: u32) -> anyhow::Result<Self> {
        let mut certs = certs.into_iter();
        let leaf = certs.next().ok_or_else(|| anyhow!("no certificate set"))?;
        let not_after_unix = Self::not_after_unix(leaf.not_after())?;
        let mut certificate = leaf
            .to_pem()
            .map_err(|e| anyhow!("failed to encode certificate: {e}"))?;
        for (i, cert) in certs.enumerate() {
            let bytes = cert
                .to_pem()
                .map_err(|e| anyhow!("failed to encode chain certificate #{i}: {e}"))?;
            certificate.extend(bytes);
        }
        let private_key = key
            .private_key_to_der()
            .map_err(|e| anyhow!("failed to encode private key: {e}"))?;
        Ok(SiteKeyCert {
            certificate: String::from_utf8(certificate)
                .map_err(|e| anyhow!("certificate pem is not utf-8: {e}"))?,
            private_key,
            not_after_unix,
            ttl,
        })
    }

    fn not_after_unix(not_after: &Asn1TimeRef) -> anyhow::Result<i64> {
        let now =
            Asn1Time::days_from_now(0).map_err(|e| anyhow!("failed to get now datatime: {e}"))?;
        let diff = now
            .diff(not_after)
            .map_err(|e| anyhow!("failed to get time diff: {e}"))?;
        if diff.days < 0 {
            return Err(anyhow!("the predefined certificate is already expired"));
        }
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        Ok(now_unix + diff.days as i64 * 86400 + diff.secs as i64)
    }
}

impl SiteConfig {
    pub(crate) fn id(&self) -> &NodeName {
        &self.id
    }

    pub(crate) fn position(&self) -> Option<&YamlDocPosition> {
        self.position.as_ref()
    }

    #[cfg(test)]
    pub(super) fn without_position(mut self) -> Self {
        self.position = None;
        self
    }

    pub(crate) fn load_at_position(position: &YamlDocPosition) -> anyhow::Result<Self> {
        let doc = vey_yaml::load_doc(position)
            .with_context(|| format!("failed to load site config at {position}"))?;
        let Yaml::Mapping(map) = doc else {
            return Err(anyhow!("yaml doc {position} is not a map"));
        };
        Self::parse(&map, Some(position.clone()))
    }

    pub(super) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut id = None;
        let config_dir;
        let lookup_dir = match &position {
            Some(position) => position.path.parent(),
            None => {
                config_dir = vey_daemon::config::get_lookup_dir(None)?;
                Some(config_dir)
            }
        };
        let mut tls_certs = None;
        let mut tls_key = None;
        let mut sign_certs = None;
        let mut sign_key = None;
        let mut enc_certs = None;
        let mut enc_key = None;
        let mut ttl = None;
        vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
            "id" => {
                if id.is_some() {
                    return Err(anyhow!("duplicate key {k}"));
                }
                id = Some(vey_yaml::value::as_metric_node_name(v)?);
                Ok(())
            }
            "ttl" => {
                if ttl.is_some() {
                    return Err(anyhow!("duplicate key {k}"));
                }
                let duration = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid duration value for key {k}"))?;
                let seconds = u32::try_from(duration.as_secs())
                    .map_err(|_| anyhow!("ttl for key {k} is too large"))?;
                ttl = Some(seconds);
                Ok(())
            }
            "certificate" | "cert" => {
                if tls_certs.is_some() {
                    return Err(anyhow!("duplicate key {k}"));
                }
                tls_certs = Some(
                    vey_yaml::value::as_openssl_certificates(v, lookup_dir)
                        .context(format!("invalid certificates value for key {k}"))?,
                );
                Ok(())
            }
            "private_key" | "key" => {
                if tls_key.is_some() {
                    return Err(anyhow!("duplicate key {k}"));
                }
                tls_key = Some(
                    vey_yaml::value::as_openssl_private_key(v, lookup_dir)
                        .context(format!("invalid private key value for key {k}"))?,
                );
                Ok(())
            }
            "sign_certificate" | "sign_cert" => {
                if sign_certs.is_some() {
                    return Err(anyhow!("duplicate key {k}"));
                }
                sign_certs = Some(
                    vey_yaml::value::as_openssl_certificates(v, lookup_dir)
                        .context(format!("invalid certificates value for key {k}"))?,
                );
                Ok(())
            }
            "sign_private_key" | "sign_key" => {
                if sign_key.is_some() {
                    return Err(anyhow!("duplicate key {k}"));
                }
                sign_key = Some(
                    vey_yaml::value::as_openssl_private_key(v, lookup_dir)
                        .context(format!("invalid private key value for key {k}"))?,
                );
                Ok(())
            }
            "enc_certificate" | "enc_cert" => {
                if enc_certs.is_some() {
                    return Err(anyhow!("duplicate key {k}"));
                }
                enc_certs = Some(
                    vey_yaml::value::as_openssl_certificates(v, lookup_dir)
                        .context(format!("invalid certificates value for key {k}"))?,
                );
                Ok(())
            }
            "enc_private_key" | "enc_key" => {
                if enc_key.is_some() {
                    return Err(anyhow!("duplicate key {k}"));
                }
                enc_key = Some(
                    vey_yaml::value::as_openssl_private_key(v, lookup_dir)
                        .context(format!("invalid private key value for key {k}"))?,
                );
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        })?;
        let Some(id) = id else {
            return Err(anyhow!("id is not set"));
        };
        let ttl = ttl.unwrap_or(24 * 3600);
        let tls = SiteKeyCert::new(tls_certs, tls_key, ttl, "tls")?.map(Arc::new);
        let tlcp_sign = SiteKeyCert::new(sign_certs, sign_key, ttl, "tlcp sign")?.map(Arc::new);
        let tlcp_enc = SiteKeyCert::new(enc_certs, enc_key, ttl, "tlcp enc")?.map(Arc::new);
        if tls.is_none() && tlcp_sign.is_none() && tlcp_enc.is_none() {
            return Err(anyhow!(
                "site {} has no tls, tlcp sign, or tlcp enc certificate",
                id.as_str()
            ));
        }
        Ok(SiteConfig {
            id,
            position,
            tls,
            tlcp_sign,
            tlcp_enc,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use saphyr::LoadableYamlNode;

    use super::*;

    fn pem_body(pem: &str) -> String {
        pem.lines().map(|line| format!("  {line}\n")).collect()
    }

    fn cert_and_key() -> (&'static str, &'static str) {
        (
            include_str!("../../../examples/simple/G3-test.crt"),
            include_str!("../../../examples/simple/G3-test.key"),
        )
    }

    fn yaml_map(text: &str) -> saphyr::Mapping<'_> {
        let mut docs = Yaml::load_from_str(text).unwrap();
        let Yaml::Mapping(map) = docs.remove(0) else {
            panic!("site yaml should be a map");
        };
        map
    }

    fn parse_text(text: &str) -> anyhow::Result<SiteConfig> {
        let dir = std::env::temp_dir();
        let position = YamlDocPosition {
            path: dir.join("site.yaml"),
            index: 0,
        };
        SiteConfig::parse(&yaml_map(text), Some(position))
    }

    fn assert_err_contains(result: anyhow::Result<SiteConfig>, needle: &str) {
        match result {
            Err(err) => {
                let message = format!("{err:#}");
                assert!(
                    message.contains(needle),
                    "error `{message}` should contain {needle}"
                );
            }
            Ok(_) => panic!("expected error containing {needle}"),
        }
    }

    #[test]
    fn parse_rejects_incomplete_site_maps() {
        assert_err_contains(parse_text("ttl: 10\n"), "id is not set");
        let (cert, key) = cert_and_key();
        assert_err_contains(
            parse_text(&format!("id: shop\ncertificate: |\n{}", pem_body(cert))),
            "tls certificate and private key must both be set",
        );
        assert_err_contains(
            parse_text(&format!("id: shop\nprivate_key: |\n{}", pem_body(key))),
            "tls certificate and private key must both be set",
        );
        assert_err_contains(parse_text("id: shop\n"), "has no tls");
        assert_err_contains(
            parse_text(&format!(
                "id: shop\nname: extra\ncertificate: |\n{}private_key: |\n{}",
                pem_body(cert),
                pem_body(key)
            )),
            "key name",
        );
        assert_err_contains(
            parse_text(&format!(
                "id: shop\nttl: 5000000000\ncertificate: |\n{}private_key: |\n{}",
                pem_body(cert),
                pem_body(key)
            )),
            "too large",
        );
    }

    #[test]
    fn parse_accepts_each_certificate_usage() {
        let (cert, key) = cert_and_key();
        let config = parse_text(&format!(
            "id: shop\ncert: |\n{}key: |\n{}sign_cert: |\n{}sign_key: |\n{}enc_cert: |\n{}enc_key: |\n{}",
            pem_body(cert),
            pem_body(key),
            pem_body(cert),
            pem_body(key),
            pem_body(cert),
            pem_body(key)
        ))
        .unwrap();
        assert!(config.tls.as_ref().unwrap().ttl().unwrap() > 0);
        assert!(
            openssl::pkey::PKey::private_key_from_der(config.tls.as_ref().unwrap().private_key())
                .is_ok()
        );
        assert!(config.tlcp_sign.is_some());
        assert!(config.tlcp_enc.is_some());
    }

    #[test]
    fn load_at_position_records_id_and_position() {
        let dir = std::env::temp_dir().join(format!(
            "vey-dcgen-site-cfg-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let cert = include_str!("../../../examples/simple/G3-test.crt");
        let key = include_str!("../../../examples/simple/G3-test.key");
        let body = format!(
            "id: shop\nttl: 120\ncertificate: |\n{}private_key: |\n{}",
            cert.lines()
                .map(|line| format!("  {line}\n"))
                .collect::<String>(),
            key.lines()
                .map(|line| format!("  {line}\n"))
                .collect::<String>()
        );
        let path = dir.join("any-name.yaml");
        fs::write(&path, body).unwrap();
        let position = YamlDocPosition { path, index: 0 };

        let config = SiteConfig::load_at_position(&position).unwrap();
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(config.id().as_str(), "shop");
        let position = config.position().unwrap();
        assert!(position.path.is_absolute());
        assert_eq!(position.index, 0);
        assert_eq!(config.tls.unwrap().ttl().unwrap(), 120);
        assert!(config.tlcp_sign.is_none());
        assert!(config.tlcp_enc.is_none());
    }
}
