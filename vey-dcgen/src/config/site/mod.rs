/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use saphyr::Yaml;

use vey_types::metrics::NodeName;
use vey_yaml::HybridParser;

mod config;
pub(crate) use config::SiteConfig;

mod registry;
pub(crate) use registry::{clear, get_all};

pub(super) fn load_one(name: &str) -> anyhow::Result<SiteConfig> {
    let id = name
        .parse::<NodeName>()
        .map_err(|e| anyhow!("invalid site id {name}: {e}"))?;
    let Some(old) = registry::get(&id) else {
        return Err(anyhow!("no site {id}"));
    };
    let Some(position) = old.position() else {
        return Err(anyhow!(
            "no config position for site {id}, reload is not supported"
        ));
    };
    let config = SiteConfig::load_at_position(position)?;
    if config.id() != &id {
        return Err(anyhow!(
            "site at position {position} has id {}, while we expect {id}",
            config.id()
        ));
    }
    let _ = registry::add(config.clone());
    Ok(config)
}

pub(super) fn load_all(value: &Yaml) -> anyhow::Result<()> {
    let conf_dir = vey_daemon::opts::config_dir()
        .ok_or_else(|| anyhow!("no valid config dir has been set"))?;
    let parser = HybridParser::new(conf_dir, vey_daemon::opts::config_file_extension());
    parser.foreach_map(value, |map, position| {
        let config = SiteConfig::parse(map, position)?;
        if let Some(old) = registry::add(config) {
            Err(anyhow!("site {} already exists", old.id()))
        } else {
            Ok(())
        }
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn site_yaml(id: &str, ttl: u32) -> String {
        let cert = include_str!("../../../examples/simple/G3-test.crt");
        let key = include_str!("../../../examples/simple/G3-test.key");
        let cert = cert
            .lines()
            .map(|line| format!("    {line}\n"))
            .collect::<String>();
        let key = key
            .lines()
            .map(|line| format!("    {line}\n"))
            .collect::<String>();
        format!(
            "id: {id}\nttl: {ttl}\ntls_server:\n  certificate: |\n{cert}  private_key: |\n{key}"
        )
    }

    fn write_site(dir: &std::path::Path, id: &str, ttl: u32) -> SiteConfig {
        let path = dir.join(format!("{id}.yaml"));
        fs::write(&path, site_yaml(id, ttl)).unwrap();
        SiteConfig::load_at_position(&vey_yaml::YamlDocPosition { path, index: 0 }).unwrap()
    }

    fn expect_err(result: anyhow::Result<SiteConfig>, needle: &str) {
        match result {
            Err(err) => assert!(
                err.to_string().contains(needle),
                "error `{err}` should contain {needle}"
            ),
            Ok(_) => panic!("expected error containing {needle}"),
        }
    }

    #[test]
    fn load_one_requires_a_recorded_file_and_matching_id() {
        expect_err(load_one(""), "invalid site id");
        expect_err(load_one("cov-missing"), "no site");

        let dir = std::env::temp_dir().join(format!(
            "vey-dcgen-site-reload-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let id = format!("cov{}", std::process::id());
        let config = write_site(&dir, &id, 40);
        expect_err(load_one(&id), "no site");

        let _ = registry::add(config.without_position());
        expect_err(load_one(&id), "reload is not supported");

        let config = write_site(&dir, &id, 40);
        let position = config.position().unwrap().clone();
        let _ = registry::add(config);
        fs::write(&position.path, site_yaml("other", 10)).unwrap();
        expect_err(load_one(&id), "while we expect");

        fs::write(&position.path, site_yaml(&id, 15)).unwrap();
        let loaded = load_one(&id).unwrap();
        assert_eq!(loaded.ttl(), 15);
        assert!(loaded.tls_server().is_some());
        let _ = fs::remove_dir_all(&dir);
    }
}
