/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::anyhow;
use openssl::x509::X509;

use vey_types::net::{OpensslCertificatePair, TlsCertUsage};

use crate::config::site::SiteConfig;

/// One certificate pair returned by the protocol. The chain is PEM and the
/// private key is DER.
pub(crate) struct SiteKeyCert {
    certificate: String,
    private_key: Vec<u8>,
    ttl: u32,
}

impl SiteKeyCert {
    pub(crate) fn certificate(&self) -> &str {
        &self.certificate
    }

    pub(crate) fn private_key(&self) -> &[u8] {
        &self.private_key
    }

    pub(crate) fn ttl(&self) -> u32 {
        self.ttl
    }

    fn from_pair(pair: &OpensslCertificatePair, ttl: u32) -> anyhow::Result<Self> {
        let leaf = X509::from_der(pair.leaf_cert())
            .map_err(|e| anyhow!("failed to decode certificate: {e}"))?;
        let mut certificate = leaf
            .to_pem()
            .map_err(|e| anyhow!("failed to encode certificate: {e}"))?;
        for (i, der) in pair.chain_certs().iter().enumerate() {
            let cert = X509::from_der(der)
                .map_err(|e| anyhow!("failed to decode chain certificate #{i}: {e}"))?;
            let bytes = cert
                .to_pem()
                .map_err(|e| anyhow!("failed to encode chain certificate #{i}: {e}"))?;
            certificate.extend(bytes);
        }
        Ok(SiteKeyCert {
            certificate: String::from_utf8(certificate)
                .map_err(|e| anyhow!("certificate pem is not utf-8: {e}"))?,
            private_key: pair.private_key().to_vec(),
            ttl,
        })
    }
}

/// Runtime site. The certificate chain is PEM and the private key is DER.
pub(crate) struct Site {
    tls_server: Option<Arc<SiteKeyCert>>,
    tlcp_server_sign: Option<Arc<SiteKeyCert>>,
    tlcp_server_enc: Option<Arc<SiteKeyCert>>,
}

impl Site {
    pub(super) fn from_config(config: &SiteConfig) -> anyhow::Result<Self> {
        let ttl = config.ttl();
        let cert = |pair: Option<&OpensslCertificatePair>| {
            pair.map(|pair| SiteKeyCert::from_pair(pair, ttl).map(Arc::new))
                .transpose()
        };
        Ok(Site {
            tls_server: cert(config.tls_server())?,
            tlcp_server_sign: cert(config.tlcp_server_sign())?,
            tlcp_server_enc: cert(config.tlcp_server_enc())?,
        })
    }

    pub(super) fn cert(&self, usage: TlsCertUsage) -> Option<Arc<SiteKeyCert>> {
        match usage {
            TlsCertUsage::TlsServer | TlsCertUsage::TLsServerTongsuo => self.tls_server.clone(),
            TlsCertUsage::TlcpServerSignature => self.tlcp_server_sign.clone(),
            TlsCertUsage::TlcpServerEncryption => self.tlcp_server_enc.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use vey_types::net::TlsCertUsage;
    use vey_yaml::YamlDocPosition;

    use super::*;

    fn pair_block(name: &str, pem: &str, key: &str) -> String {
        let cert = pem
            .lines()
            .map(|line| format!("    {line}\n"))
            .collect::<String>();
        let key = key
            .lines()
            .map(|line| format!("    {line}\n"))
            .collect::<String>();
        format!("{name}:\n  certificate: |\n{cert}  private_key: |\n{key}")
    }

    #[test]
    fn open_uses_position_and_validates_certificates() {
        let dir = std::env::temp_dir().join(format!(
            "vey-dcgen-site-rt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shop.yaml");
        let cert = include_str!("../../examples/simple/G3-test.crt");
        let key = include_str!("../../examples/simple/G3-test.key");
        fs::write(
            &path,
            format!(
                "id: shop\n{}\n{}\n{}",
                pair_block("tls_server", cert, key),
                pair_block("tlcp_server_sign", cert, key),
                pair_block("tlcp_server_enc", cert, key)
            ),
        )
        .unwrap();
        let position = YamlDocPosition {
            path: path.clone(),
            index: 0,
        };
        let config = SiteConfig::load_at_position(&position).unwrap();
        let site = Site::from_config(&config).unwrap();
        let _ = fs::remove_dir_all(&dir);

        let tls = site.cert(TlsCertUsage::TlsServer).unwrap();
        assert!(tls.certificate().contains("BEGIN CERTIFICATE"));
        assert!(openssl::pkey::PKey::private_key_from_der(tls.private_key()).is_ok());
        assert_eq!(tls.ttl(), 24 * 3600);
        assert!(site.cert(TlsCertUsage::TlcpServerSignature).is_some());
        assert!(site.cert(TlsCertUsage::TlcpServerEncryption).is_some());
    }
}
