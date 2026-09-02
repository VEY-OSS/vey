/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::FromStr;

use anyhow::anyhow;

use vey_types::metrics::NodeName;
use vey_yaml::YamlDocPosition;

pub(in crate::control) async fn reload() -> anyhow::Result<()> {
    vey_daemon::runtime::main_handle()
        .ok_or(anyhow!("unable to get main runtime handle"))?
        .spawn(crate::signal::reload())
        .await
        .map_err(|e| anyhow!("failed to spawn reload task: {e}"))?
}

macro_rules! impl_reload {
    ($f:ident, $m:tt) => {
        pub(in crate::control) async fn $f(
            name: String,
            position: Option<YamlDocPosition>,
        ) -> anyhow::Result<()> {
            let name = NodeName::from_str(name.as_str())
                .map_err(|e| anyhow!("invalid node name {name}: {e}"))?;
            vey_daemon::runtime::main_handle()
                .ok_or(anyhow!("unable to get main runtime handle"))?
                .spawn(async move { crate::$m::reload(&name, position).await })
                .await
                .map_err(|e| anyhow!("failed to spawn reload task: {e}"))?
        }
    };
}

impl_reload!(reload_user_group, auth);
impl_reload!(reload_auditor, audit);
impl_reload!(reload_resolver, resolve);
impl_reload!(reload_escaper, escape);
impl_reload!(reload_server, serve);
impl_reload!(reload_site_group, site);
