/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

mod redirect;
mod strategy;

pub use redirect::{ResolveRedirection, ResolveRedirectionBuilder, ResolveRedirectionValue};
pub use strategy::{PickStrategy, QueryStrategy, ResolveStrategy};
