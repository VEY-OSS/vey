/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::rc::Rc;

use vey_dcgen_proto::proc_capnp::proc_control;

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

    async fn reload_issuer(
        self: Rc<Self>,
        _params: proc_control::ReloadIssuerParams,
        mut results: proc_control::ReloadIssuerResults,
    ) -> capnp::Result<()> {
        let r = crate::signal::reload_issuer().await;
        set_operation_result(results.get().init_result(), r);
        Ok(())
    }
}
