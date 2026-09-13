/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod h1;
mod h2;

pub(crate) use h1::SiteHttp1Pool;
pub(crate) use h2::SiteHttp2Pool;

use vey_types::metrics::NodeName;

/// Pick the unaided-worker lane. Out-of-range and missing ids use lane 0.
fn lane_index(worker_id: Option<usize>, lane_count: usize) -> usize {
    match worker_id {
        Some(id) if id < lane_count => id,
        _ => 0,
    }
}

/// Shared H1/H2 origin-pool checkout key: TLS and the server's configured escaper.
///
/// Connections opened through different server escapers must not be mixed.
/// Servers that use the same escaper may reuse across server instances.
fn origin_matches(
    is_tls: bool,
    escaper: &NodeName,
    want_tls: bool,
    want_escaper: &NodeName,
) -> bool {
    is_tls == want_tls && escaper == want_escaper
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn name(s: &str) -> NodeName {
        NodeName::from_str(s).unwrap()
    }

    #[test]
    fn lane_stays_on_worker_id() {
        assert_eq!(lane_index(Some(0), 4), 0);
        assert_eq!(lane_index(Some(3), 4), 3);
        assert_eq!(lane_index(Some(4), 4), 0);
        assert_eq!(lane_index(None, 4), 0);
        assert_eq!(lane_index(None, 1), 0);
    }

    #[test]
    fn origin_key_matches_tls_and_escaper() {
        let a = name("direct_a");
        let b = name("direct_b");
        assert!(origin_matches(true, &a, true, &a));
        assert!(origin_matches(false, &a, false, &a));
        assert!(!origin_matches(true, &a, false, &a));
        assert!(!origin_matches(true, &a, true, &b));
    }

    #[test]
    fn origin_select_leaves_other_escapers() {
        let a = name("direct_a");
        let b = name("direct_b");
        let mut idle = vec![(true, a.clone()), (true, b.clone()), (true, a.clone())];
        let pos = idle
            .iter()
            .rposition(|(tls, e)| origin_matches(*tls, e, true, &a))
            .unwrap();
        assert_eq!(pos, 2);
        idle.remove(pos);
        assert!(idle.iter().any(|(_, e)| e == &b));
        assert!(idle.iter().any(|(_, e)| e == &a));
    }
}
