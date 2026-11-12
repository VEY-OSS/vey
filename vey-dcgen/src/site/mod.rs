/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use vey_types::metrics::NodeName;
use vey_types::net::TlsCertUsage;

use crate::config::site::{SiteConfig, SiteKeyCert};

mod registry;

mod site;
pub(crate) use site::Site;

pub fn load_all() {
    let sites = crate::config::site::get_all()
        .iter()
        .map(|config| (config.id().clone(), Arc::new(Site::from_config(config))))
        .collect();
    registry::replace(sites);
}

pub(crate) fn load_one(config: &SiteConfig) {
    let id = config.id().clone();
    let site = Arc::new(Site::from_config(config));
    registry::insert(id, site);
}

pub(crate) fn get(id: &str, usage: TlsCertUsage) -> Option<Arc<SiteKeyCert>> {
    let id = id.parse::<NodeName>().ok()?;
    registry::get(&id)?.cert(usage)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use vey_types::net::TlsCertUsage;
    use vey_yaml::YamlDocPosition;

    use crate::config::site::SiteConfig;

    use super::*;

    fn pem_body(pem: &str) -> String {
        pem.lines().map(|line| format!("  {line}\n")).collect()
    }

    #[test]
    fn get_selects_the_configured_usage() {
        let dir = std::env::temp_dir().join(format!(
            "vey-dcgen-site-get-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let id = format!("get{}", std::process::id());
        let cert = include_str!("../../examples/simple/G3-test.crt");
        let key = include_str!("../../examples/simple/G3-test.key");
        let path = dir.join("site.yaml");
        fs::write(
            &path,
            format!(
                "id: {id}\nttl: 30\ncertificate: |\n{}private_key: |\n{}sign_certificate: |\n{}sign_private_key: |\n{}",
                pem_body(cert),
                pem_body(key),
                pem_body(cert),
                pem_body(key)
            ),
        )
        .unwrap();
        let config = SiteConfig::load_at_position(&YamlDocPosition { path, index: 0 }).unwrap();
        load_one(&config);
        let _ = fs::remove_dir_all(&dir);

        let tls = get(&id, TlsCertUsage::TlsServer).unwrap();
        assert!(tls.certificate().contains("BEGIN CERTIFICATE"));
        assert!(openssl::pkey::PKey::private_key_from_der(tls.private_key()).is_ok());
        assert_eq!(tls.ttl().unwrap(), 30);
        assert!(get(&id, TlsCertUsage::TLsServerTongsuo).is_some());
        assert!(get(&id, TlsCertUsage::TlcpServerSignature).is_some());
        assert!(get(&id, TlsCertUsage::TlcpServerEncryption).is_none());
        assert!(get("missing-site", TlsCertUsage::TlsServer).is_none());
        assert!(get("", TlsCertUsage::TlsServer).is_none());
    }
}
