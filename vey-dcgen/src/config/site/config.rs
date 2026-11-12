/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::{Context, anyhow};
use saphyr::Yaml;

use vey_types::metrics::NodeName;
use vey_types::net::OpensslCertificatePair;
use vey_yaml::YamlDocPosition;

/// Parsed site. `position` is set when the map was loaded from a file.
#[derive(Clone)]
pub(crate) struct SiteConfig {
    id: NodeName,
    position: Option<YamlDocPosition>,
    ttl: u32,
    tls_server: Option<OpensslCertificatePair>,
    tlcp_server_sign: Option<OpensslCertificatePair>,
    tlcp_server_enc: Option<OpensslCertificatePair>,
}

impl SiteConfig {
    const DEFAULT_TTL: u32 = 24 * 3600;

    fn new(position: Option<YamlDocPosition>) -> Self {
        SiteConfig {
            id: NodeName::default(),
            position,
            ttl: Self::DEFAULT_TTL,
            tls_server: None,
            tlcp_server_sign: None,
            tlcp_server_enc: None,
        }
    }

    pub(crate) fn id(&self) -> &NodeName {
        &self.id
    }

    pub(crate) fn position(&self) -> Option<&YamlDocPosition> {
        self.position.as_ref()
    }

    pub(crate) fn ttl(&self) -> u32 {
        self.ttl
    }

    pub(crate) fn tls_server(&self) -> Option<&OpensslCertificatePair> {
        self.tls_server.as_ref()
    }

    pub(crate) fn tlcp_server_sign(&self) -> Option<&OpensslCertificatePair> {
        self.tlcp_server_sign.as_ref()
    }

    pub(crate) fn tlcp_server_enc(&self) -> Option<&OpensslCertificatePair> {
        self.tlcp_server_enc.as_ref()
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
        let mut site = Self::new(position);
        vey_yaml::foreach_kv(map, |k, v| site.set(k, v))?;
        site.check()?;
        Ok(site)
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "id" => {
                if !self.id.is_empty() {
                    return Err(anyhow!("duplicate key {k}"));
                }
                self.id = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "ttl" => {
                let duration = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid duration value for key {k}"))?;
                self.ttl = u32::try_from(duration.as_secs())
                    .map_err(|_| anyhow!("ttl for key {k} is too large"))?;
                Ok(())
            }
            "tls_server" => {
                if self.tls_server.is_some() {
                    return Err(anyhow!("duplicate key {k}"));
                }
                let lookup_dir = vey_daemon::config::get_lookup_dir(self.position.as_ref())?;
                let tls = vey_yaml::value::as_openssl_certificate_pair(v, Some(lookup_dir))
                    .context(format!("invalid certificate pair value for key {k}"))?;
                self.tls_server = Some(tls);
                Ok(())
            }
            "tlcp_server_sign" => {
                if self.tlcp_server_sign.is_some() {
                    return Err(anyhow!("duplicate key {k}"));
                }
                let lookup_dir = vey_daemon::config::get_lookup_dir(self.position.as_ref())?;
                let sign = vey_yaml::value::as_openssl_certificate_pair(v, Some(lookup_dir))
                    .context(format!("invalid certificate pair value for key {k}"))?;
                self.tlcp_server_sign = Some(sign);
                Ok(())
            }
            "tlcp_server_enc" => {
                if self.tlcp_server_enc.is_some() {
                    return Err(anyhow!("duplicate key {k}"));
                }
                let lookup_dir = vey_daemon::config::get_lookup_dir(self.position.as_ref())?;
                let enc = vey_yaml::value::as_openssl_certificate_pair(v, Some(lookup_dir))
                    .context(format!("invalid certificate pair value for key {k}"))?;
                self.tlcp_server_enc = Some(enc);
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    fn check(&self) -> anyhow::Result<()> {
        if self.id.is_empty() {
            return Err(anyhow!("id is not set"));
        }
        if self.tls_server.is_none()
            && self.tlcp_server_sign.is_none()
            && self.tlcp_server_enc.is_none()
        {
            return Err(anyhow!(
                "site {} has no tls_server, tlcp_server_sign, or tlcp_server_enc certificate",
                self.id
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use saphyr::LoadableYamlNode;

    use super::*;

    fn pem_at(pem: &str, spaces: usize) -> String {
        let pad = " ".repeat(spaces);
        pem.lines().map(|line| format!("{pad}{line}\n")).collect()
    }

    fn pair_block(name: &str, cert: &str, key: &str) -> String {
        format!(
            "{name}:\n  certificate: |\n{}  private_key: |\n{}",
            pem_at(cert, 4),
            pem_at(key, 4)
        )
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
            parse_text(&format!(
                "id: shop\ntls_server:\n  certificate: |\n{}",
                pem_at(cert, 4)
            )),
            "no private key set",
        );
        assert_err_contains(
            parse_text(&format!(
                "id: shop\ntls_server:\n  private_key: |\n{}",
                pem_at(key, 4)
            )),
            "no certificate set",
        );
        assert_err_contains(parse_text("id: shop\n"), "tls_server");
        assert_err_contains(
            parse_text(&format!(
                "id: shop\nname: extra\n{}",
                pair_block("tls_server", cert, key)
            )),
            "key name",
        );
        assert_err_contains(
            parse_text(&format!(
                "id: shop\nttl: 5000000000\n{}",
                pair_block("tls_server", cert, key)
            )),
            "too large",
        );
    }

    #[test]
    fn parse_accepts_each_certificate_usage() {
        let (cert, key) = cert_and_key();
        let config = parse_text(&format!(
            "id: shop\n{}\n{}\n{}",
            pair_block("tls_server", cert, key),
            pair_block("tlcp_server_sign", cert, key),
            pair_block("tlcp_server_enc", cert, key)
        ))
        .unwrap();
        assert_eq!(config.ttl(), SiteConfig::DEFAULT_TTL);
        assert!(
            openssl::pkey::PKey::private_key_from_der(config.tls_server().unwrap().private_key())
                .is_ok()
        );
        assert!(config.tlcp_server_sign().is_some());
        assert!(config.tlcp_server_enc().is_some());
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
            "id: shop\nttl: 120\n{}",
            pair_block("tls_server", cert, key)
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
        assert_eq!(config.ttl(), 120);
        assert!(config.tls_server().is_some());
        assert!(config.tlcp_server_sign().is_none());
        assert!(config.tlcp_server_enc().is_none());
    }
}
