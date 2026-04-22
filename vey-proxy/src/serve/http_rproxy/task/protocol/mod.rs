/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use vey_io_ext::{LimitedBufReader, LimitedWriter};

mod request;

pub(super) use request::HttpRProxyRequest;

pub(super) type HttpClientReader<CDR> = LimitedBufReader<CDR>;
pub(super) type HttpClientWriter<CDW> = LimitedWriter<CDW>;
