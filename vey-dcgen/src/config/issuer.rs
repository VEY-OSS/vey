/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::sync::Arc;

use anyhow::{Context, anyhow};
use arc_swap::ArcSwapOption;
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;
use yaml_rust::Yaml;

use vey_histogram::HistogramMetricsConfig;

static ISSUER_CONFIG: ArcSwapOption<OpensslIssuerConfig> = ArcSwapOption::const_empty();

pub(crate) fn get_config() -> Option<Arc<OpensslIssuerConfig>> {
    ISSUER_CONFIG.load_full()
}

pub(crate) fn replace_config(mut config: OpensslIssuerConfig) {
    if let Some(current) = get_config() {
        config.duration_stats = current.duration_stats.clone();
    }
    ISSUER_CONFIG.store(Some(Arc::new(config)));
}

pub(crate) struct OpensslIssuerConfig {
    pub(crate) ca_cert: X509,
    pub(crate) ca_key: PKey<Private>,
    pub(crate) ca_cert_pem: Vec<u8>,
    pub(crate) keep_serial: bool,
    pub(crate) max_ttl: i32,
    pub(crate) duration_stats: HistogramMetricsConfig,
}

pub(super) fn load_config(value: &Yaml) -> anyhow::Result<()> {
    if get_config().is_some() {
        return Err(anyhow!("duplicate issuer config"));
    }
    replace_config(parse_config(value)?);
    Ok(())
}

pub(super) fn parse_config(value: &Yaml) -> anyhow::Result<OpensslIssuerConfig> {
    if let Yaml::Hash(map) = value {
        let mut no_append_ca_cert = false;
        let mut ca_cert_pem = Vec::new();
        let mut ca_cert: Option<X509> = None;
        let mut ca_key: Option<PKey<Private>> = None;
        let mut keep_serial = false;
        let mut max_ttl = 24 * 3600; // 1 day
        let mut duration_stats = HistogramMetricsConfig::default();
        let lookup_dir = vey_daemon::config::get_lookup_dir(None)?;

        vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
            "ca_certificate" => {
                let certs = vey_yaml::value::as_openssl_certificates(v, Some(lookup_dir))
                    .context(format!("invalid openssl certificate value for key {k}"))?;
                for (i, cert) in certs.iter().enumerate() {
                    let pem = cert.to_pem().map_err(|e| {
                        anyhow!("failed to convert cert {i} back to pem format: {e}")
                    })?;
                    ca_cert_pem.extend(pem);
                }

                let cert = certs
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow!("no valid openssl certificate key found"))?;
                ca_cert = Some(cert);
                Ok(())
            }
            "ca_private_key" => {
                let key = vey_yaml::value::as_openssl_private_key(v, Some(lookup_dir))
                    .context(format!("invalid openssl private key value for key {k}"))?;
                ca_key = Some(key);
                Ok(())
            }
            "no_append_ca_cert" => {
                no_append_ca_cert = vey_yaml::value::as_bool(v)?;
                Ok(())
            }
            "keep_serial" => {
                keep_serial = vey_yaml::value::as_bool(v)?;
                Ok(())
            }
            "max_ttl" => {
                let v = vey_yaml::value::as_i32(v)?;
                max_ttl = v.max(300); // at least for 5 minutes
                Ok(())
            }
            "duration_stats" | "duration_metrics" => {
                duration_stats = vey_yaml::value::as_histogram_metrics_config(v).context(
                    format!("invalid histogram metrics config value for key {k}"),
                )?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        })?;

        let Some(ca_cert) = ca_cert else {
            return Err(anyhow!("no ca certificate set"));
        };
        let Some(ca_key) = ca_key else {
            return Err(anyhow!("no ca private key set"));
        };

        if no_append_ca_cert {
            ca_cert_pem.clear();
        }
        Ok(OpensslIssuerConfig {
            ca_cert,
            ca_key,
            ca_cert_pem,
            keep_serial,
            max_ttl,
            duration_stats,
        })
    } else {
        Err(anyhow!(
            "yaml value type for the issuer config should be 'map'"
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use vey_histogram::HistogramMetricsConfig;

    use super::*;

    fn example_config(max_ttl: i32) -> OpensslIssuerConfig {
        let cert_pem = include_bytes!("../../examples/simple/G3-test.crt");
        let key_pem = include_bytes!("../../examples/simple/G3-test.key");
        OpensslIssuerConfig {
            ca_cert: X509::from_pem(cert_pem).unwrap(),
            ca_key: PKey::private_key_from_pem(key_pem).unwrap(),
            ca_cert_pem: cert_pem.to_vec(),
            keep_serial: false,
            max_ttl,
            duration_stats: HistogramMetricsConfig::default(),
        }
    }

    #[test]
    fn parse_rejects_non_map() {
        match parse_config(&Yaml::Integer(1)) {
            Err(err) => assert!(err.to_string().contains("map")),
            Ok(_) => panic!("non-map issuer config was accepted"),
        }
    }

    #[test]
    fn replace_swaps_signing_options_and_keeps_duration_stats() {
        let mut initial = example_config(400);
        initial.duration_stats = HistogramMetricsConfig::with_rotate(Duration::from_secs(9));
        let expected_duration = initial.duration_stats.clone();
        replace_config(initial);
        replace_config(example_config(900));

        let current = get_config().unwrap();
        assert_eq!(current.max_ttl, 900);
        assert_eq!(current.duration_stats, expected_duration);
        assert!(!current.ca_cert_pem.is_empty());
    }
}
