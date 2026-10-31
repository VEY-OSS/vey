/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::anyhow;
use log::{debug, info, warn};
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;
use tokio::runtime::Handle;

use vey_cert_agent::Request;
use vey_histogram::{HistogramRecorder, HistogramStats};
use vey_tls_cert::builder::{MimicCertBuilder, ServerCertBuilder, TlsServerCertBuilder};
use vey_tls_cert::ext::X509Ext;
use vey_types::net::TlsCertUsage;

use crate::config::OpensslIssuerConfig;

static REQUEST_SENDER: OnceLock<kanal::AsyncSender<CertRequest>> = OnceLock::new();
static DURATION_RECORDER: OnceLock<HistogramRecorder<u64>> = OnceLock::new();
static DURATION_STATS: OnceLock<Arc<HistogramStats>> = OnceLock::new();
static ISSUER_STATS: OnceLock<Arc<IssuerStats>> = OnceLock::new();

pub(crate) struct GeneratedData {
    pub(crate) cert: String,
    pub(crate) key: Vec<u8>,
    pub(crate) ttl: u32,
}

pub(crate) struct IssuedCert {
    pub(crate) user_req: Request,
    pub(crate) cert: String,
    pub(crate) key: Vec<u8>,
    pub(crate) ttl: u32,
}

pub(crate) enum GenerateError {
    Queue(anyhow::Error),
    Failed,
    Dropped,
}

struct CertRequest {
    user_req: Request,
    reply: tokio::sync::oneshot::Sender<Result<IssuedCert, ()>>,
}

pub(crate) fn stats() -> Option<Arc<IssuerStats>> {
    ISSUER_STATS.get().cloned()
}

pub(crate) fn duration_stats() -> Option<Arc<HistogramStats>> {
    DURATION_STATS.get().cloned()
}

pub(crate) fn duration_recorder() -> &'static HistogramRecorder<u64> {
    DURATION_RECORDER.get().expect("issuer is not running")
}

pub(crate) async fn generate(user_req: Request) -> Result<IssuedCert, GenerateError> {
    let (reply, response) = tokio::sync::oneshot::channel();
    REQUEST_SENDER
        .get()
        .expect("issuer is not running")
        .send(CertRequest { user_req, reply })
        .await
        .map_err(|e| GenerateError::Queue(anyhow!("failed to send request to issuer: {e}")))?;
    match response.await {
        Ok(Ok(issued)) => Ok(issued),
        Ok(Err(())) => Err(GenerateError::Failed),
        Err(_) => Err(GenerateError::Dropped),
    }
}

pub(crate) fn spawn_workers() -> anyhow::Result<()> {
    let issuer_config =
        crate::config::get_issuer_config().ok_or_else(|| anyhow!("no issuer config available"))?;
    let issuer_stats = Arc::new(IssuerStats::default());
    let (duration_recorder, duration_stats) = issuer_config.duration_stats.build_spawned(None);
    let (req_sender, req_receiver) = kanal::bounded_async::<CertRequest>(1024);

    let workers = vey_daemon::runtime::worker::foreach(|h| {
        let issuer = OpensslIssuer::new(&issuer_stats)
            .map_err(|e| anyhow!("failed to build issuer for worker {}: {e}", h.id))?;
        issuer.spawn(&h.handle, h.id, req_receiver.clone());
        Ok::<(), anyhow::Error>(())
    })?;
    if workers < 1 {
        let issuer = OpensslIssuer::new(&issuer_stats)
            .map_err(|e| anyhow!("failed to build issuer for main runtime: {e}"))?;
        issuer.spawn(&Handle::current(), 0, req_receiver);
    }

    REQUEST_SENDER
        .set(req_sender)
        .map_err(|_| anyhow!("issuer is already running"))?;
    DURATION_RECORDER
        .set(duration_recorder)
        .map_err(|_| anyhow!("issuer is already running"))?;
    DURATION_STATS
        .set(duration_stats)
        .map_err(|_| anyhow!("issuer is already running"))?;
    ISSUER_STATS
        .set(issuer_stats)
        .map_err(|_| anyhow!("issuer is already running"))?;
    Ok(())
}

mod stats;
pub(crate) use stats::IssuerStats;

pub(crate) struct OpensslIssuer {
    builder: ServerCertBuilder,
    stats: Arc<IssuerStats>,
}

impl OpensslIssuer {
    pub(crate) fn new(stats: &Arc<IssuerStats>) -> anyhow::Result<Self> {
        let builder = TlsServerCertBuilder::new_ec256()?;
        Ok(OpensslIssuer {
            builder,
            stats: Arc::clone(stats),
        })
    }

    pub(crate) fn refresh(&mut self) -> anyhow::Result<()> {
        self.stats.add_refresh_total();
        self.builder.refresh_datetime()?;
        self.builder.refresh_ec256()?;
        self.stats.add_refresh_ok();
        Ok(())
    }

    fn generate(&mut self, req: &Request) -> anyhow::Result<GeneratedData> {
        let config = crate::config::get_issuer_config()
            .ok_or_else(|| anyhow!("no issuer config available"))?;
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

    fn spawn(
        mut self,
        handle: &Handle,
        id: usize,
        req_receiver: kanal::AsyncReceiver<CertRequest>,
    ) {
        handle.spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = self.refresh() {
                            warn!("failed to refresh issuer: {e:?}");
                        }
                    }
                    r = req_receiver.recv() => {
                        let Ok(req) = r else {
                            break
                        };

                        if let Some(cert) = req.user_req.cert() {
                            info!("{}", cert.dump());
                        }

                        let host = req.user_req.host().clone();
                        debug!("{host} - [#{id}] start cert generation");
                        let issued = match self.generate(&req.user_req) {
                            Ok(data) => {
                                debug!("{host} - [#{id}] cert generated");
                                Ok(IssuedCert {
                                    user_req: req.user_req,
                                    cert: data.cert,
                                    key: data.key,
                                    ttl: data.ttl,
                                })
                            }
                            Err(e) => {
                                warn!("{host} - [#{id}] cert generation failed: {e:?}");
                                Err(())
                            }
                        };
                        let _ = req.reply.send(issued);
                    }
                }
            }
        });
    }
}
