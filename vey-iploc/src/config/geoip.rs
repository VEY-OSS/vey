/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2024-2025 ByteDance and/or its affiliates.
 */

use std::path::Path;
use std::sync::Arc;

use anyhow::anyhow;
use yaml_rust::Yaml;

pub(crate) fn load(v: &Yaml, conf_dir: &Path) -> anyhow::Result<()> {
    if let Yaml::Hash(map) = v {
        vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
            "country" => {
                let path = vey_yaml::value::as_file_path(v, conf_dir, false)?;
                let db = vey_geoip_db::file::load_country(&path)?;
                vey_geoip_db::store::store_country(Arc::new(db));
                Ok(())
            }
            "asn" => {
                let path = vey_yaml::value::as_file_path(v, conf_dir, false)?;
                let db = vey_geoip_db::file::load_asn(&path)?;
                vey_geoip_db::store::store_asn(Arc::new(db));
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        })
    } else {
        Err(anyhow!("invalid value type"))
    }
}
