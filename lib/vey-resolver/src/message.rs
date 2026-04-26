/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use tokio::sync::oneshot;

use vey_types::net::DomainName;

use super::{ArcResolvedRecord, ResolvedRecord, ResolvedRecordSource, ResolverConfig};

#[derive(Clone, Debug)]
pub(crate) enum ResolverCommand {
    Quit,
    Update(Box<ResolverConfig>),
}

pub(crate) enum ResolveDriverRequest {
    GetV4(
        DomainName,
        oneshot::Sender<(ArcResolvedRecord, ResolvedRecordSource)>,
    ),
    GetV6(
        DomainName,
        oneshot::Sender<(ArcResolvedRecord, ResolvedRecordSource)>,
    ),
}

pub(crate) enum ResolveDriverResponse {
    V4(ResolvedRecord),
    V6(ResolvedRecord),
}
