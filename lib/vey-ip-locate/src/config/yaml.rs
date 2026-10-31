/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#[cfg(unix)]
use std::path::PathBuf;

use anyhow::{Context, anyhow};
use yaml_rust::Yaml;

use super::QueryPeer;
use crate::IpLocateServiceConfig;

impl QueryPeer {
    fn parse_udp_yaml(v: &Yaml) -> anyhow::Result<Self> {
        match v {
            Yaml::Hash(map) => {
                let mut addr = None;

                vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
                    "address" | "addr" => {
                        addr = Some(vey_yaml::value::as_env_sockaddr(v).context(format!(
                            "invalid query peer socket address value for key {k}"
                        ))?);
                        Ok(())
                    }
                    _ => Err(anyhow!("invalid key {k}")),
                })?;

                if let Some(addr) = addr.take() {
                    Ok(QueryPeer::Udp(addr))
                } else {
                    Err(anyhow!("no target address has been set"))
                }
            }
            Yaml::String(_) => {
                let addr = vey_yaml::value::as_env_sockaddr(v)?;
                Ok(QueryPeer::Udp(addr))
            }
            _ => Err(anyhow!("invalid yaml value for udp query peer")),
        }
    }

    #[cfg(unix)]
    fn parse_unix_yaml(v: &Yaml) -> anyhow::Result<Self> {
        match v {
            Yaml::Hash(map) => {
                let mut path: Option<PathBuf> = None;

                vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
                    "path" => {
                        path = Some(
                            vey_yaml::value::as_absolute_path(v)
                                .context(format!("invalid value for key {k}"))?,
                        );
                        Ok(())
                    }
                    _ => Err(anyhow!("invalid key {k}")),
                })?;
                if let Some(path) = path.take() {
                    Ok(QueryPeer::Unix(path))
                } else {
                    Err(anyhow!("no path has been set"))
                }
            }
            Yaml::String(_) => {
                let path = vey_yaml::value::as_absolute_path(v)?;
                Ok(QueryPeer::Unix(path))
            }
            _ => Err(anyhow!("invalid yaml value for unix query peer")),
        }
    }
}

impl IpLocateServiceConfig {
    fn set_query_peer_addr_by_yaml(&mut self, value: &Yaml) -> anyhow::Result<()> {
        let addr = vey_yaml::value::as_env_sockaddr(value)?;
        self.set_query_peer_addr(addr);
        Ok(())
    }

    fn set_query_peer_by_yaml_kv(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "udp" => {
                let peer =
                    QueryPeer::parse_udp_yaml(v).context(format!("invalid value for key {k}"))?;
                self.set_query_peer(peer);
                Ok(())
            }
            #[cfg(unix)]
            "unix" => {
                let peer =
                    QueryPeer::parse_unix_yaml(v).context(format!("invalid value for key {k}"))?;
                self.set_query_peer(peer);
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    pub fn parse_yaml(value: &Yaml) -> anyhow::Result<Self> {
        match value {
            Yaml::Hash(map) => {
                let mut config = IpLocateServiceConfig::default();

                vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
                    "cache_request_batch_count" => {
                        let count = vey_yaml::value::as_nonzero_usize(v)?;
                        config.set_cache_request_batch_count(count);
                        Ok(())
                    }
                    "cache_request_timeout" => {
                        let time = vey_yaml::humanize::as_duration(v)
                            .context(format!("invalid humanize duration value for key {k}"))?;
                        config.set_cache_request_timeout(time);
                        Ok(())
                    }
                    "query_peer_addr" => {
                        config
                            .set_query_peer_addr_by_yaml(v)
                            .context(format!("invalid sockaddr str value for key {k}"))?;
                        Ok(())
                    }
                    "query_peer_udp" => {
                        let peer = QueryPeer::parse_udp_yaml(v)
                            .context(format!("invalid value for key {k}"))?;
                        config.set_query_peer(peer);
                        Ok(())
                    }
                    #[cfg(unix)]
                    "query_peer_unix" => {
                        let peer = QueryPeer::parse_unix_yaml(v)
                            .context(format!("invalid value for key {k}"))?;
                        config.set_query_peer(peer);
                        Ok(())
                    }
                    "query_peer" => {
                        if let Yaml::Hash(map) = v {
                            vey_yaml::foreach_kv(map, |k, v| {
                                config.set_query_peer_by_yaml_kv(k, v)
                            })
                            .context(format!("invalid value for key {k}"))?;
                            Ok(())
                        } else {
                            Err(anyhow!("yaml value type for key {k} should be 'map'"))
                        }
                    }
                    "query_socket_buffer" => {
                        let buf_config = vey_yaml::value::as_socket_buffer_config(v)
                            .context(format!("invalid socket buffer config value for key {k}"))?;
                        config.set_query_socket_buffer(buf_config);
                        Ok(())
                    }
                    "query_wait_timeout" => {
                        let time = vey_yaml::humanize::as_duration(v)
                            .context(format!("invalid humanize duration value for key {k}"))?;
                        config.set_query_wait_timeout(time);
                        Ok(())
                    }
                    "default_expire_ttl" => {
                        let ttl = vey_yaml::value::as_u32(v)?;
                        config.set_default_expire_ttl(ttl);
                        Ok(())
                    }
                    "maximum_expire_ttl" => {
                        let ttl = vey_yaml::value::as_u32(v)?;
                        config.set_maximum_expire_ttl(ttl);
                        Ok(())
                    }
                    _ => Err(anyhow!("invalid key {k}")),
                })?;

                Ok(config)
            }
            Yaml::String(_) => {
                let mut config = IpLocateServiceConfig::default();
                config
                    .set_query_peer_addr_by_yaml(value)
                    .context("invalid sockaddr str value")?;
                Ok(config)
            }
            _ => Err(anyhow!(
                "yaml type for 'ip location service config' should be 'map'"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use vey_yaml::{yaml_doc, yaml_str};

    fn udp(addr: &str) -> QueryPeer {
        QueryPeer::Udp(addr.parse::<SocketAddr>().unwrap())
    }

    #[test]
    fn parse_map_ok() {
        let yaml = yaml_doc!(
            r#"
                cache_request_batch_count: 20
                cache_request_timeout: "5s"
                query_peer_addr: "192.168.1.1:2888"
                query_socket_buffer:
                  recv: 65536
                  send: 32768
                query_wait_timeout: "2s"
                default_expire_ttl: 30
                maximum_expire_ttl: 600
            "#
        );
        let config = IpLocateServiceConfig::parse_yaml(&yaml).unwrap();
        assert_eq!(config.cache_request_batch_count.get(), 20);
        assert_eq!(
            config.cache_request_timeout,
            std::time::Duration::from_secs(5)
        );
        assert_eq!(config.query_peer, udp("192.168.1.1:2888"));
        assert_eq!(config.query_socket_buffer.recv_size(), Some(65536));
        assert_eq!(config.query_socket_buffer.send_size(), Some(32768));
        assert_eq!(config.query_wait_timeout, std::time::Duration::from_secs(2));
        assert_eq!(config.default_expire_ttl, 30);
        assert_eq!(config.maximum_expire_ttl, 600);
    }

    #[test]
    fn parse_map_err() {
        let yaml = yaml_doc!(
            r#"
                invalid_key: "value"
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = yaml_doc!(
            r#"
                cache_request_batch_count: -1
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = yaml_doc!(
            r#"
                cache_request_timeout: "5x"
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = yaml_doc!(
            r#"
                query_peer_addr: "invalid_address"
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = yaml_doc!(
            r#"
                query_socket_buffer:
                  recv: -1024
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = yaml_doc!(
            r#"
                query_wait_timeout: "-1s"
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = yaml_doc!(
            r#"
                default_expire_ttl: -10
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = yaml_doc!(
            r#"
                maximum_expire_ttl: NaN
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());
    }

    #[test]
    fn parse_string() {
        // Valid address
        let yaml = yaml_str!("127.0.0.1:3000");
        let config = IpLocateServiceConfig::parse_yaml(&yaml).unwrap();
        assert_eq!(config.query_peer, udp("127.0.0.1:3000"));

        // Invalid address
        let yaml = yaml_str!("invalid-address");
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());
    }

    #[test]
    fn parse_invalid_yaml_types() {
        let yaml = Yaml::Array(vec![]);
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = Yaml::Integer(123);
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = Yaml::Boolean(true);
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = Yaml::Real("1.23".to_string());
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = Yaml::Null;
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());
    }

    #[test]
    fn parse_query_peer_udp() {
        let yaml = yaml_doc!(
            r#"
                query_peer_udp: "10.0.0.1:2888"
            "#
        );
        let config = IpLocateServiceConfig::parse_yaml(&yaml).unwrap();
        assert_eq!(config.query_peer, udp("10.0.0.1:2888"));

        let yaml = yaml_doc!(
            r#"
                query_peer_udp:
                  address: "10.0.0.2:2888"
            "#
        );
        let config = IpLocateServiceConfig::parse_yaml(&yaml).unwrap();
        assert_eq!(config.query_peer, udp("10.0.0.2:2888"));

        let yaml = yaml_doc!(
            r#"
                query_peer:
                  udp:
                    addr: "10.0.0.3:2888"
            "#
        );
        let config = IpLocateServiceConfig::parse_yaml(&yaml).unwrap();
        assert_eq!(config.query_peer, udp("10.0.0.3:2888"));

        let yaml = yaml_doc!(
            r#"
                query_peer_addr: "192.168.1.1:2888"
                query_peer:
                  udp: "10.0.0.4:2888"
            "#
        );
        let config = IpLocateServiceConfig::parse_yaml(&yaml).unwrap();
        assert_eq!(config.query_peer, udp("10.0.0.4:2888"));

        let yaml = yaml_doc!(
            r#"
                query_peer: "127.0.0.1:2888"
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = yaml_doc!(
            r#"
                query_peer_udp:
                  address: "invalid"
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = yaml_doc!(
            r#"
                query_peer_udp: 123
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = yaml_doc!(
            r#"
                query_peer_udp: {}
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn parse_query_peer_unix() {
        let yaml = yaml_doc!(
            r#"
                query_peer_unix: "/run/vey-iploc.sock"
            "#
        );
        let config = IpLocateServiceConfig::parse_yaml(&yaml).unwrap();
        assert_eq!(
            config.query_peer,
            QueryPeer::Unix("/run/vey-iploc.sock".into())
        );

        let yaml = yaml_doc!(
            r#"
                query_peer_unix:
                  path: "/run/vey-iploc/query.sock"
            "#
        );
        let config = IpLocateServiceConfig::parse_yaml(&yaml).unwrap();
        assert_eq!(
            config.query_peer,
            QueryPeer::Unix("/run/vey-iploc/query.sock".into())
        );

        let yaml = yaml_doc!(
            r#"
                query_peer:
                  unix: "/tmp/vey-iploc.sock"
            "#
        );
        let config = IpLocateServiceConfig::parse_yaml(&yaml).unwrap();
        assert_eq!(
            config.query_peer,
            QueryPeer::Unix("/tmp/vey-iploc.sock".into())
        );

        let yaml = yaml_doc!(
            r#"
                query_peer_unix: "/run/vey-iploc.sock"
                query_peer_addr: "127.0.0.1:2888"
            "#
        );
        let config = IpLocateServiceConfig::parse_yaml(&yaml).unwrap();
        assert_eq!(config.query_peer, udp("127.0.0.1:2888"));

        let yaml = yaml_doc!(
            r#"
                query_peer_unix: "relative/path"
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = yaml_doc!(
            r#"
                query_peer_unix: 123
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = yaml_doc!(
            r#"
                query_peer_unix: {}
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());

        let yaml = yaml_doc!(
            r#"
                query_peer:
                  other: "/tmp/vey-iploc.sock"
            "#
        );
        assert!(IpLocateServiceConfig::parse_yaml(&yaml).is_err());
    }
}
