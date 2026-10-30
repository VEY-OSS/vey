/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::IpAddr;
use std::sync::LazyLock;

use arc_swap::ArcSwap;
use ip_network_table::IpNetworkTable;

use vey_geoip_db::{GeoIpAsnRecord, GeoIpCountryRecord};
use vey_geoip_types::IpLocationBuilder;

static COUNTRY: LazyLock<ArcSwap<IpNetworkTable<GeoIpCountryRecord>>> =
    LazyLock::new(|| ArcSwap::from_pointee(IpNetworkTable::new()));
static ASN: LazyLock<ArcSwap<IpNetworkTable<GeoIpAsnRecord>>> =
    LazyLock::new(|| ArcSwap::from_pointee(IpNetworkTable::new()));

pub(crate) fn load() {
    load_country();
    load_asn();
}

pub(crate) fn load_country() {
    if let Some(db) = crate::config::geoip::loaded_country() {
        COUNTRY.store(db);
    }
}

pub(crate) fn load_asn() {
    if let Some(db) = crate::config::geoip::loaded_asn() {
        ASN.store(db);
    }
}

pub(crate) fn lookup_country(ip: IpAddr, location: &mut IpLocationBuilder) {
    let db = COUNTRY.load();
    let Some((net, record)) = db.longest_match(ip) else {
        return;
    };
    location.set_network(net);
    location.set_country(record.country);
    location.set_continent(record.continent);
}

pub(crate) fn lookup_asn(ip: IpAddr, location: &mut IpLocationBuilder) {
    let db = ASN.load();
    let Some((net, record)) = db.longest_match(ip) else {
        return;
    };
    location.set_network(net);
    location.set_as_number(record.number);
    if let Some(name) = record.isp_name() {
        location.set_isp_name(name.to_owned());
    }
    if let Some(domain) = record.isp_domain() {
        location.set_isp_domain(domain.to_owned());
    }
}
