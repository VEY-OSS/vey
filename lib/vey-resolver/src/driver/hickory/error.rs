/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2024-2025 ByteDance and/or its affiliates.
 * Copyright 2026 VEY-OSS developers.
 */

use hickory_proto::ProtoError;
use hickory_proto::op::ResponseCode;

use crate::error::{ResolveDriverError, ResolveError, ResolveServerError};

impl ResolveError {
    pub(super) fn from_response_code(code: ResponseCode) -> Option<Self> {
        match code {
            ResponseCode::NoError => None,
            ResponseCode::FormErr => Some(ResolveServerError::FormErr.into()),
            ResponseCode::ServFail => Some(ResolveServerError::ServFail.into()),
            ResponseCode::NXDomain => Some(ResolveServerError::NotFound.into()),
            ResponseCode::NotImp => Some(ResolveServerError::NotImp.into()),
            ResponseCode::Refused => Some(ResolveServerError::Refused.into()),
            ResponseCode::BADNAME => Some(ResolveDriverError::BadName.into()),
            _ => Some(ResolveDriverError::BadResp.into()),
        }
    }
}

impl From<ProtoError> for ResolveError {
    fn from(value: ProtoError) -> Self {
        let e = match value {
            ProtoError::NotAResponse => ResolveDriverError::BadResp,
            ProtoError::FormError { .. } => ResolveDriverError::BadResp,
            ProtoError::CharacterDataTooLong { .. } => ResolveDriverError::BadResp,
            ProtoError::Decode(_) => ResolveDriverError::BadResp,
            v => ResolveDriverError::Internal(v.to_string()),
        };
        ResolveError::FromDriver(e)
    }
}
