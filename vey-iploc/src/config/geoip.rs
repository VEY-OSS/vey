/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use ip_network_table::IpNetworkTable;
use yaml_rust::Yaml;

use vey_geoip_db::{GeoIpAsnRecord, GeoIpCountryRecord};

type CountryDb = Arc<IpNetworkTable<GeoIpCountryRecord>>;
type AsnDb = Arc<IpNetworkTable<GeoIpAsnRecord>>;

struct CountrySource {
    path: PathBuf,
    db: CountryDb,
}

struct AsnSource {
    path: PathBuf,
    db: AsnDb,
}

static COUNTRY: Mutex<Option<CountrySource>> = Mutex::new(None);
static ASN: Mutex<Option<AsnSource>> = Mutex::new(None);

pub(crate) fn load(v: &Yaml, conf_dir: &Path) -> anyhow::Result<()> {
    let Yaml::Hash(map) = v else {
        return Err(anyhow!("invalid value type"));
    };

    let mut country = None;
    let mut asn = None;
    vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
        "country" => {
            let path = vey_yaml::value::as_file_path(v, conf_dir, false)?;
            let db = Arc::new(vey_geoip_db::file::load_country(&path)?);
            country = Some(CountrySource { path, db });
            Ok(())
        }
        "asn" => {
            let path = vey_yaml::value::as_file_path(v, conf_dir, false)?;
            let db = Arc::new(vey_geoip_db::file::load_asn(&path)?);
            asn = Some(AsnSource { path, db });
            Ok(())
        }
        _ => Err(anyhow!("invalid key {k}")),
    })?;

    if country.is_some() {
        *COUNTRY.lock().unwrap() = country;
    }
    if asn.is_some() {
        *ASN.lock().unwrap() = asn;
    }
    Ok(())
}

pub(crate) fn loaded_country() -> Option<CountryDb> {
    COUNTRY
        .lock()
        .unwrap()
        .as_ref()
        .map(|source| Arc::clone(&source.db))
}

pub(crate) fn loaded_asn() -> Option<AsnDb> {
    ASN.lock()
        .unwrap()
        .as_ref()
        .map(|source| Arc::clone(&source.db))
}

pub(crate) fn reload_country() -> anyhow::Result<()> {
    let path = COUNTRY
        .lock()
        .unwrap()
        .as_ref()
        .map(|source| source.path.clone())
        .ok_or_else(|| anyhow!("country database path is not configured"))?;
    let db = Arc::new(vey_geoip_db::file::load_country(&path).map_err(|e| {
        anyhow!(
            "failed to reload country database {}: {e:#}",
            path.display()
        )
    })?);
    if let Some(source) = COUNTRY.lock().unwrap().as_mut() {
        source.db = db;
    }
    Ok(())
}

pub(crate) fn reload_asn() -> anyhow::Result<()> {
    let path = ASN
        .lock()
        .unwrap()
        .as_ref()
        .map(|source| source.path.clone())
        .ok_or_else(|| anyhow!("asn database path is not configured"))?;
    let db = Arc::new(
        vey_geoip_db::file::load_asn(&path)
            .map_err(|e| anyhow!("failed to reload asn database {}: {e:#}", path.display()))?,
    );
    if let Some(source) = ASN.lock().unwrap().as_mut() {
        source.db = db;
    }
    Ok(())
}
