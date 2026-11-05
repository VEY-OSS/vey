/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use std::str::FromStr;

use vey_types::net::Interface;

use anyhow::anyhow;
use saphyr::{Scalar, Yaml};

pub fn as_interface(value: &Yaml) -> anyhow::Result<Interface> {
    match value {
        Yaml::Value(Scalar::String(s)) => {
            Interface::from_str(s).map_err(|e| anyhow!("invalid interface name {s}: {e}"))
        }
        Yaml::Value(Scalar::Integer(i)) => {
            let u = u32::try_from(*i).map_err(|_| anyhow!("out of range u32 value {}", *i))?;
            Interface::try_from(u).map_err(|e| anyhow!("invalid interface id {u}: {e}"))
        }
        _ => Err(anyhow!(
            "yaml value type for 'InterfaceName' should be 'string' or 'u32'"
        )),
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "linux", target_os = "android"))]
    const LOOPBACK_INTERFACE: &str = "lo";
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    const LOOPBACK_INTERFACE: &str = "lo0";

    #[test]
    fn as_interface_ok() {
        let yaml = yaml_str!(LOOPBACK_INTERFACE);
        assert_eq!(as_interface(&yaml).unwrap().name(), LOOPBACK_INTERFACE);

        let yaml = Yaml::Value(Scalar::Integer(1));
        let interface = as_interface(&yaml).unwrap();
        assert_eq!(interface.id().get(), 1);
    }

    #[test]
    fn as_interface_err() {
        let yaml = yaml_str!("invalid_interface");
        assert!(as_interface(&yaml).is_err());

        let yaml = Yaml::Value(Scalar::Integer(u32::MAX as i64 + 1));
        assert!(as_interface(&yaml).is_err());

        let yaml = Yaml::Value(Scalar::Integer(0));
        assert!(as_interface(&yaml).is_err());

        let yaml = Yaml::Value(Scalar::Boolean(true));
        assert!(as_interface(&yaml).is_err());
    }
}
