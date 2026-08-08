/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use openssl::asn1::Asn1Integer;
use openssl::bn::{BigNum, MsbOption};

pub fn random_16() -> anyhow::Result<Asn1Integer> {
    let mut bn = BigNum::new().map_err(|e| anyhow!("failed to create big num: {e}"))?;
    bn.rand(128, MsbOption::ONE, true)
        .map_err(|e| anyhow!("failed to generate random big num: {e}"))?;
    bn.to_asn1_integer()
        .map_err(|e| anyhow!("failed to convert bn to asn1 integer: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_16_is_positive() {
        let serial = random_16().unwrap();
        assert!(serial.to_bn().unwrap().num_bits() > 0);
    }

    #[test]
    fn random_16_unique() {
        let a = random_16().unwrap();
        let b = random_16().unwrap();
        assert_ne!(a.to_bn().unwrap(), b.to_bn().unwrap());
    }
}
