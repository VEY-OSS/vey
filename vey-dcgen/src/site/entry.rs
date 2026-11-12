/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use vey_types::net::TlsCertUsage;

use crate::config::site::{SiteConfig, SiteKeyCert};

/// Runtime site. The certificate chain is PEM and the private key is DER.
pub(crate) struct Site {
    tls: Option<Arc<SiteKeyCert>>,
    tlcp_sign: Option<Arc<SiteKeyCert>>,
    tlcp_enc: Option<Arc<SiteKeyCert>>,
}

impl Site {
    pub(super) fn from_config(config: &SiteConfig) -> Self {
        Site {
            tls: config.tls.clone(),
            tlcp_sign: config.tlcp_sign.clone(),
            tlcp_enc: config.tlcp_enc.clone(),
        }
    }

    pub(super) fn cert(&self, usage: TlsCertUsage) -> Option<Arc<SiteKeyCert>> {
        match usage {
            TlsCertUsage::TlsServer | TlsCertUsage::TLsServerTongsuo => self.tls.clone(),
            TlsCertUsage::TlcpServerSignature => self.tlcp_sign.clone(),
            TlsCertUsage::TlcpServerEncryption => self.tlcp_enc.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use vey_types::net::TlsCertUsage;
    use vey_yaml::YamlDocPosition;

    use super::*;

    fn indent_pem(pem: &str) -> String {
        pem.lines().map(|line| format!("  {line}\n")).collect()
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
                "id: shop\ncertificate: |\n{}private_key: |\n{}sign_certificate: |\n{}sign_private_key: |\n{}enc_certificate: |\n{}enc_private_key: |\n{}",
                indent_pem(cert),
                indent_pem(key),
                indent_pem(cert),
                indent_pem(key),
                indent_pem(cert),
                indent_pem(key)
            ),
        )
        .unwrap();
        let position = YamlDocPosition {
            path: path.clone(),
            index: 0,
        };
        let config = SiteConfig::load_at_position(&position).unwrap();
        let site = Site::from_config(&config);
        let _ = fs::remove_dir_all(&dir);

        let tls = site.cert(TlsCertUsage::TlsServer).unwrap();
        assert!(tls.certificate().contains("BEGIN CERTIFICATE"));
        assert!(openssl::pkey::PKey::private_key_from_der(tls.private_key()).is_ok());
        assert!(tls.ttl().unwrap() > 0);
        assert!(site.cert(TlsCertUsage::TlcpServerSignature).is_some());
        assert!(site.cert(TlsCertUsage::TlcpServerEncryption).is_some());
    }
}
