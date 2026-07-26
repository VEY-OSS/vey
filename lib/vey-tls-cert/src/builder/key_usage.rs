/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use openssl::error::ErrorStack;
use openssl::x509::X509Extension;
use openssl::x509::extension::KeyUsage;

pub struct KeyUsageBuilder(KeyUsage);

impl KeyUsageBuilder {
    pub fn ca() -> Self {
        let mut usage = KeyUsage::new();
        usage.critical().key_cert_sign().crl_sign();
        KeyUsageBuilder(usage)
    }

    pub fn tls_general() -> Self {
        let mut usage = KeyUsage::new();
        usage
            .critical()
            .key_agreement()
            .digital_signature()
            .key_encipherment();
        KeyUsageBuilder(usage)
    }

    /// Edwards-curve Digital Signature Algorithm
    pub fn ed_dsa() -> Self {
        let mut usage = KeyUsage::new();
        usage.critical().digital_signature();
        KeyUsageBuilder(usage)
    }

    /// for CurveXXX for Diffie-Hellman Key Exchange
    pub fn x_dh() -> Self {
        let mut usage = KeyUsage::new();
        usage.critical().key_agreement();
        KeyUsageBuilder(usage)
    }

    pub fn tlcp_sign() -> Self {
        let mut usage = KeyUsage::new();
        usage.critical().non_repudiation().digital_signature();
        KeyUsageBuilder(usage)
    }

    pub fn tlcp_enc() -> Self {
        let mut usage = KeyUsage::new();
        usage
            .critical()
            .key_agreement()
            .key_encipherment()
            .data_encipherment();
        KeyUsageBuilder(usage)
    }

    pub fn build(&self) -> Result<X509Extension, ErrorStack> {
        self.0.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_key_usage_builds(builder: KeyUsageBuilder) {
        let ext = builder.build().unwrap();
        assert!(!ext.to_der().unwrap().is_empty());
    }

    #[test]
    fn ca_usage_builds_critical_extension() {
        assert_key_usage_builds(KeyUsageBuilder::ca());
    }

    #[test]
    fn tls_general_usage_builds_critical_extension() {
        assert_key_usage_builds(KeyUsageBuilder::tls_general());
    }

    #[test]
    fn ed_dsa_usage_builds_critical_extension() {
        assert_key_usage_builds(KeyUsageBuilder::ed_dsa());
    }

    #[test]
    fn x_dh_usage_builds_critical_extension() {
        assert_key_usage_builds(KeyUsageBuilder::x_dh());
    }

    #[test]
    fn tlcp_usages_build_distinct_extensions() {
        let sign = KeyUsageBuilder::tlcp_sign().build().unwrap();
        let enc = KeyUsageBuilder::tlcp_enc().build().unwrap();
        assert_ne!(sign.to_der().unwrap(), enc.to_der().unwrap());
    }
}
