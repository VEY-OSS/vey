/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::{Arc, LazyLock};

use arc_swap::ArcSwapOption;
use ip_network_table::IpNetworkTable;

use crate::{GeoIpAsnRecord, GeoIpCountryRecord};

static GEO_COUNTRY_DB: LazyLock<ArcSwapOption<IpNetworkTable<GeoIpCountryRecord>>> =
    LazyLock::new(|| ArcSwapOption::new(None));
static GEO_ASN_DB: LazyLock<ArcSwapOption<IpNetworkTable<GeoIpAsnRecord>>> =
    LazyLock::new(|| ArcSwapOption::new(None));

pub fn load_country() -> Option<Arc<IpNetworkTable<GeoIpCountryRecord>>> {
    GEO_COUNTRY_DB.load_full()
}

pub fn store_country(db: Arc<IpNetworkTable<GeoIpCountryRecord>>) {
    GEO_COUNTRY_DB.store(Some(db));
}

pub fn load_asn() -> Option<Arc<IpNetworkTable<GeoIpAsnRecord>>> {
    GEO_ASN_DB.load_full()
}

pub fn store_asn(db: Arc<IpNetworkTable<GeoIpAsnRecord>>) {
    GEO_ASN_DB.store(Some(db));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;
    use std::str::FromStr;

    use ip_network::IpNetwork;
    use vey_geoip_types::{ContinentCode, IsoCountryCode};

    #[test]
    fn store_and_load_country() {
        let mut table = IpNetworkTable::new();
        let network = IpNetwork::new(IpAddr::from_str("10.0.0.0").unwrap(), 8).unwrap();
        table.insert(
            network,
            GeoIpCountryRecord {
                country: IsoCountryCode::US,
                continent: ContinentCode::NA,
            },
        );
        store_country(Arc::new(table));

        let loaded = load_country().expect("country db should be stored");
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let record = loaded.longest_match(ip).unwrap();
        assert_eq!(record.1.country, IsoCountryCode::US);
    }

    #[test]
    fn store_and_load_asn() {
        let mut table = IpNetworkTable::new();
        let network = IpNetwork::new(IpAddr::from_str("192.168.0.0").unwrap(), 16).unwrap();
        table.insert(
            network,
            GeoIpAsnRecord {
                number: 64512,
                name: None,
                domain: None,
            },
        );
        store_asn(Arc::new(table));

        let loaded = load_asn().expect("asn db should be stored");
        let ip: IpAddr = "192.168.1.1".parse().unwrap();
        let record = loaded.longest_match(ip).unwrap();
        assert_eq!(record.1.number, 64512);
    }
}
