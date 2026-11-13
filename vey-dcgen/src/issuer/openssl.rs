/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::anyhow;
use log::{debug, info, warn};
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;

use vey_cert_agent::Request;
use vey_tls_cert::builder::{MimicCertBuilder, ServerCertBuilder, TlsServerCertBuilder};
use vey_tls_cert::ext::X509Ext;
use vey_types::net::TlsCertUsage;

use crate::config::issuer::OpensslIssuerConfig;

use super::GeneratedData;
use super::stats::IssuerStats;

const REFRESH_INTERVAL: Duration = Duration::from_secs(300);

pub(super) struct OpensslIssuer {
    builder: ServerCertBuilder,
    stats: Arc<IssuerStats>,
    refreshed_at: Instant,
}

impl OpensslIssuer {
    pub(super) fn new(stats: &Arc<IssuerStats>) -> anyhow::Result<Self> {
        let builder = TlsServerCertBuilder::new_ec256()?;
        Ok(OpensslIssuer {
            builder,
            stats: Arc::clone(stats),
            refreshed_at: Instant::now(),
        })
    }

    pub(super) fn sign(&mut self, req: &Request) -> anyhow::Result<GeneratedData> {
        self.maybe_refresh();
        if let Some(cert) = req.cert() {
            info!("{}", cert.dump());
        }
        debug!(
            "{} - start cert generation for site {}",
            req.host(),
            req.site()
        );
        match self.generate(req) {
            Ok(data) => {
                debug!("{} - cert generated", req.host());
                Ok(data)
            }
            Err(e) => {
                warn!("{} - cert generation failed: {e:?}", req.host());
                Err(e)
            }
        }
    }

    fn maybe_refresh(&mut self) {
        if self.refreshed_at.elapsed() < REFRESH_INTERVAL {
            return;
        }
        if let Err(e) = self.refresh() {
            warn!("failed to refresh issuer: {e:?}");
            return;
        }
        self.refreshed_at = Instant::now();
    }

    fn refresh(&mut self) -> anyhow::Result<()> {
        self.stats.add_refresh_total();
        self.builder.refresh_datetime()?;
        self.builder.refresh_ec256()?;
        self.stats.add_refresh_ok();
        Ok(())
    }

    fn generate(&mut self, req: &Request) -> anyhow::Result<GeneratedData> {
        let config = super::get_config().ok_or_else(|| anyhow!("no issuer config available"))?;
        self.stats.add_request_total();
        if let Some(mimic_cert) = req.cert() {
            self.generate_mimic(&config, mimic_cert, req.cert_usage())
        } else {
            self.builder.refresh_serial()?;
            let cert =
                self.builder
                    .build_fake(req.host(), &config.ca_cert, &config.ca_key, None)?;
            let ttl = self.builder.valid_seconds()?;
            self.pack_data(&config, cert, self.builder.pkey(), ttl)
        }
    }

    fn generate_mimic(
        &self,
        config: &OpensslIssuerConfig,
        mimic_cert: &X509,
        cert_usage: TlsCertUsage,
    ) -> anyhow::Result<GeneratedData> {
        let mut mimic_builder = MimicCertBuilder::new(mimic_cert)?;
        mimic_builder.set_keep_serial(config.keep_serial);

        let cert = match cert_usage {
            TlsCertUsage::TlsServer => {
                mimic_builder.build_tls_cert(&config.ca_cert, &config.ca_key, None)?
            }
            TlsCertUsage::TLsServerTongsuo => mimic_builder.build_tls_cert_with_new_usage(
                &config.ca_cert,
                &config.ca_key,
                None,
            )?,
            TlsCertUsage::TlcpServerEncryption => {
                mimic_builder.build_tlcp_enc_cert(&config.ca_cert, &config.ca_key, None)?
            }
            TlsCertUsage::TlcpServerSignature => {
                mimic_builder.build_tlcp_sign_cert(&config.ca_cert, &config.ca_key, None)?
            }
        };

        let ttl = mimic_builder.valid_seconds()?;

        self.pack_data(config, cert, mimic_builder.pkey(), ttl)
    }

    fn pack_data(
        &self,
        config: &OpensslIssuerConfig,
        cert: X509,
        pkey: &PKey<Private>,
        ttl: i32,
    ) -> anyhow::Result<GeneratedData> {
        let ttl = ttl.clamp(0, config.max_ttl) as u32;
        let mut cert_pem = cert
            .to_pem()
            .map_err(|e| anyhow!("failed to encode cert to PEM format: {e}"))?;
        if !config.ca_cert_pem.is_empty() {
            cert_pem.extend_from_slice(&config.ca_cert_pem);
        }
        let key = pkey
            .private_key_to_der()
            .map_err(|e| anyhow!("failed to encode pkey to DER format: {e}"))?;

        let data = GeneratedData {
            cert: unsafe { String::from_utf8_unchecked(cert_pem) },
            key,
            ttl,
        };
        self.stats.add_request_ok();
        Ok(data)
    }
}
