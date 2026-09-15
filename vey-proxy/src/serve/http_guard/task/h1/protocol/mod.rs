/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use vey_io_ext::{LimitedBufReader, LimitedWriter};

mod request;

pub(super) use request::HttpGuardRequest;

pub(super) type HttpClientReader<CDR> = LimitedBufReader<CDR>;
pub(super) type HttpClientWriter<CDW> = LimitedWriter<CDW>;
