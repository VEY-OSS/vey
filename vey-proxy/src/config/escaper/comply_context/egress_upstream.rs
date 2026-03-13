/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 VEY-OSS developers.
 */

use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::{Context, anyhow};
use yaml_rust::Yaml;

use vey_types::net::{Host, UpstreamAddr};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EgressUpstream {
    pub(crate) addr: Option<UpstreamAddr>,
    pub(crate) resolve_sticky_key: String,
}

impl EgressUpstream {
    fn is_empty(&self) -> bool {
        self.addr.is_none() && self.resolve_sticky_key.is_empty()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct EgressUpstreamConfig {
    host_key: String,
    port_key: String,
    domain_suffix: String,
    default_port: u16,
    resolve_sticky_key: String,
}

impl EgressUpstreamConfig {
    pub(super) fn parse(value: &Yaml) -> anyhow::Result<Self> {
        let Yaml::Hash(map) = value else {
            return Err(anyhow!("egress upstream config should be a hash value"));
        };

        let mut conf = EgressUpstreamConfig::default();
        vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
            "host_key" => {
                conf.host_key = vey_yaml::value::as_string(v)?;
                Ok(())
            }
            "port_key" => {
                conf.port_key = vey_yaml::value::as_string(v)?;
                Ok(())
            }
            "domain_suffix" => {
                conf.domain_suffix = vey_yaml::value::as_domain(v)
                    .context(format!("invalid domain value for key {k}"))?;
                Ok(())
            }
            "default_port" => {
                conf.default_port = vey_yaml::value::as_u16(v)?;
                Ok(())
            }
            "resolve_sticky_key" => {
                conf.resolve_sticky_key = vey_yaml::value::as_string(v)?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        })?;

        conf.check()?;
        Ok(conf)
    }

    fn check(&self) -> anyhow::Result<()> {
        if self.default_port == 0 {
            return Err(anyhow!("no default port set"));
        }
        Ok(())
    }

    pub(crate) fn build_with_context(
        &self,
        context_kv: &BTreeMap<String, String>,
    ) -> Option<EgressUpstream> {
        let mut value = EgressUpstream {
            addr: None,
            resolve_sticky_key: String::new(),
        };

        if !self.host_key.is_empty()
            && let Some(host_name) = context_kv.get(&self.host_key)
        {
            let mut port = self.default_port;
            if !self.port_key.is_empty()
                && let Some(port_s) = context_kv.get(&self.port_key)
                && let Ok(port_v) = u16::from_str(port_s)
            {
                port = port_v;
            }

            let host = if self.domain_suffix.is_empty() {
                Host::from_str(host_name)
            } else {
                let mut host_s = host_name.to_string();
                host_s.push('.');
                host_s.push_str(&self.domain_suffix);
                Host::from_domain_str(&host_s)
            };
            if let Ok(host) = host {
                value.addr = Some(UpstreamAddr::new(host, port));
            }
        }

        if !self.resolve_sticky_key.is_empty()
            && let Some(resolve_sticky_key) = context_kv.get(&self.resolve_sticky_key)
        {
            value.resolve_sticky_key.clone_from(resolve_sticky_key);
        }

        if value.is_empty() { None } else { Some(value) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vey_yaml::yaml_doc;

    #[test]
    fn t() {
        let conf = yaml_doc!(
            r#"
            "default_port": "8080"
            "host_key": "host"
            "domain_suffix": "example.net"
            "resolve_sticky_key": "session-id"
            "#
        );
        let c = EgressUpstreamConfig::parse(&conf).unwrap();

        let mut context = BTreeMap::new();
        context.insert("host".to_string(), "test".to_string());
        context.insert("session-id".to_string(), "abcd".to_string());

        let egress_upstream = c.build_with_context(&context).unwrap();
        assert_eq!(egress_upstream.resolve_sticky_key, "abcd");
        let addr = egress_upstream.addr.unwrap();
        assert_eq!(addr.port(), 8080);
        assert_eq!(addr.host().to_string(), "test.example.net");
    }
}
