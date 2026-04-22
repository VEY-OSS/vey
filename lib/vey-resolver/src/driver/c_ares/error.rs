/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 * Copyright 2026 VEY-OSS Developers.
 */

use crate::error::{ResolveDriverErrorReason, ResolveError, ResolveServerError};

impl ResolveError {
    pub(super) fn from_cares_error(e: c_ares::Error) -> Option<ResolveError> {
        match e {
            c_ares::Error::ENODATA => None, // NODATA is not really an error
            c_ares::Error::EFORMERR => Some(ResolveServerError::FormErr.into()),
            c_ares::Error::ESERVFAIL => Some(ResolveServerError::ServFail.into()),
            c_ares::Error::ENOTFOUND => Some(ResolveServerError::NotFound.into()),
            c_ares::Error::ENOTIMP => Some(ResolveServerError::NotImp.into()),
            c_ares::Error::EREFUSED => Some(ResolveServerError::Refused.into()),
            c_ares::Error::ETIMEOUT => Some(ResolveError::DriverTimeout),
            _ => Some(ResolveDriverErrorReason::Owned(e.to_string()).into()),
        }
    }
}
