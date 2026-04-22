/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

pub mod driver;
pub use driver::AnyResolveDriverConfig;

pub(crate) use driver::{BoxResolverDriver, ResolveDriver};

mod config;
mod error;
mod handle;
mod message;
mod query;
mod record;
mod resolver;
mod runtime;
mod stats;

pub use config::{ResolverConfig, ResolverRuntimeConfig};
pub use error::{ResolveDriverErrorReason, ResolveError, ResolveServerError};
pub use handle::{ResolveJob, ResolveJobRecvResult, ResolverHandle};
pub use query::ResolveQueryType;
pub use record::{ArcResolvedRecord, ResolvedRecord, ResolvedRecordSource};
pub use resolver::{Resolver, ResolverBuilder};
pub use stats::{ResolverMemorySnapshot, ResolverQuerySnapshot, ResolverSnapshot, ResolverStats};
