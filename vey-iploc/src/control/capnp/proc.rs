/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::IpAddr;
use std::rc::Rc;
use std::str::FromStr;

use vey_iploc_proto::proc_capnp::proc_control;
use vey_iploc_proto::types_capnp::location_record;

use super::set_operation_result;

pub(super) struct ProcControlImpl;

impl proc_control::Server for ProcControlImpl {
    async fn version(
        self: Rc<Self>,
        _params: proc_control::VersionParams,
        mut results: proc_control::VersionResults,
    ) -> capnp::Result<()> {
        results.get().set_version(crate::build::VERSION);
        Ok(())
    }

    async fn offline(
        self: Rc<Self>,
        _params: proc_control::OfflineParams,
        mut results: proc_control::OfflineResults,
    ) -> capnp::Result<()> {
        vey_daemon::control::quit::start_graceful_shutdown().await;
        set_operation_result(results.get().init_result(), Ok(()));
        Ok(())
    }

    async fn cancel_shutdown(
        self: Rc<Self>,
        _params: proc_control::CancelShutdownParams,
        mut results: proc_control::CancelShutdownResults,
    ) -> capnp::Result<()> {
        let r = vey_daemon::control::quit::cancel_graceful_shutdown().await;
        set_operation_result(results.get().init_result(), r);
        Ok(())
    }

    async fn release_controller(
        self: Rc<Self>,
        _params: proc_control::ReleaseControllerParams,
        mut results: proc_control::ReleaseControllerResults,
    ) -> capnp::Result<()> {
        let r = vey_daemon::control::quit::release_controller().await;
        set_operation_result(results.get().init_result(), r);
        Ok(())
    }

    async fn reload(
        self: Rc<Self>,
        _params: proc_control::ReloadParams,
        mut results: proc_control::ReloadResults,
    ) -> capnp::Result<()> {
        let r = crate::signal::reload().await;
        set_operation_result(results.get().init_result(), r);
        Ok(())
    }

    async fn reload_country(
        self: Rc<Self>,
        _params: proc_control::ReloadCountryParams,
        mut results: proc_control::ReloadCountryResults,
    ) -> capnp::Result<()> {
        let r = crate::signal::reload_country().await;
        set_operation_result(results.get().init_result(), r);
        Ok(())
    }

    async fn reload_asn(
        self: Rc<Self>,
        _params: proc_control::ReloadAsnParams,
        mut results: proc_control::ReloadAsnResults,
    ) -> capnp::Result<()> {
        let r = crate::signal::reload_asn().await;
        set_operation_result(results.get().init_result(), r);
        Ok(())
    }

    async fn query(
        self: Rc<Self>,
        params: proc_control::QueryParams,
        mut results: proc_control::QueryResults,
    ) -> capnp::Result<()> {
        let ip = params.get()?.get_ip()?.to_str()?;
        let result = results.get().init_result();
        match IpAddr::from_str(ip) {
            Ok(ip) => match crate::serve::locate(ip) {
                Some(location) => fill_location(result.init_location(), &location),
                None => {
                    let mut err = result.init_err();
                    err.set_code(-1);
                    err.set_reason("no location found");
                    Ok(())
                }
            },
            Err(e) => {
                let mut err = result.init_err();
                err.set_code(-1);
                err.set_reason(format!("invalid ip address: {e}").as_str());
                Ok(())
            }
        }
    }
}

fn fill_location(
    mut builder: location_record::Builder<'_>,
    location: &vey_geoip_types::IpLocation,
) -> capnp::Result<()> {
    let network = location.network_addr().to_string();
    builder.set_network(network.as_str());
    if let Some(country) = location.country() {
        builder.set_country(country.alpha2_code());
    }
    if let Some(continent) = location.continent() {
        builder.set_continent(continent.code());
    }
    if let Some(asn) = location.network_asn() {
        builder.set_asn(asn);
        builder.set_asn_set(true);
    }
    if let Some(name) = location.isp_name() {
        builder.set_isp_name(name);
    }
    if let Some(domain) = location.isp_domain() {
        builder.set_isp_domain(domain);
    }
    Ok(())
}
