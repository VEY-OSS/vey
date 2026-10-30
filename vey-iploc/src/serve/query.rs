/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::IpAddr;

use log::warn;

use vey_geoip_types::{IpLocation, IpLocationBuilder};
use vey_ip_locate::{Request, Response};

use super::stats::ServerStats;

const RESPONSE_TTL: u32 = 300;

pub(super) fn build_response(packet: &[u8], stats: &ServerStats) -> Option<Vec<u8>> {
    stats.add_request_total();

    let req = match Request::parse_req(packet) {
        Ok(req) => req,
        Err(e) => {
            stats.add_request_invalid();
            warn!("invalid request: {e:?}");
            return None;
        }
    };
    let Some(ip) = req.ip() else {
        stats.add_request_invalid();
        return None;
    };
    let Some(location) = locate(ip) else {
        stats.add_lookup_not_found();
        return None;
    };

    match Response::encode_new(ip, location, RESPONSE_TTL) {
        Ok(buf) => {
            stats.add_response_total();
            Some(buf)
        }
        Err(e) => {
            warn!("failed to encode response for ip {ip}: {e}");
            None
        }
    }
}

pub(crate) fn locate(ip: IpAddr) -> Option<IpLocation> {
    let mut builder = IpLocationBuilder::default();
    crate::geoip::lookup_country(ip, &mut builder);
    crate::geoip::lookup_asn(ip, &mut builder);
    builder.build().ok()
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use std::str::FromStr;

    use vey_ip_locate::Request;
    use vey_types::metrics::NodeName;

    use super::*;

    #[test]
    fn miss_counts_lookup_not_found() {
        let stats = ServerStats::new(&NodeName::from_str("local").unwrap());
        let packet = Request::encode_new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1))).unwrap();

        assert!(build_response(&packet, &stats).is_none());
        assert_eq!(stats.take_request_total(), 1);
        assert_eq!(stats.take_lookup_not_found(), 1);
        assert_eq!(stats.take_request_invalid(), 0);
        assert_eq!(stats.take_response_total(), 0);

        assert!(build_response(b"not a request", &stats).is_none());
        assert_eq!(stats.take_lookup_not_found(), 0);
        assert_eq!(stats.take_request_invalid(), 1);
    }
}
