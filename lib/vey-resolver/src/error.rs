/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fmt;

use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ResolveServerError {
    #[error("server claims query was malformed")]
    FormErr,
    #[error("server returned general failure")]
    ServFail,
    #[error("server claims domain name not found")]
    NotFound,
    #[error("server does not implement requested operation")]
    NotImp,
    #[error("server refused query")]
    Refused,
    #[error("server returned response code {0}")]
    Other(u16),
}

impl ResolveServerError {
    pub fn get_type(&self) -> &str {
        match self {
            ResolveServerError::FormErr => "FORMERR",
            ResolveServerError::ServFail => "SERVFAIL",
            ResolveServerError::NotFound => "NOTFOUND",
            ResolveServerError::NotImp => "NOTIMP",
            ResolveServerError::Refused => "REFUSED",
            ResolveServerError::Other(_) => "OTHER",
        }
    }
}

#[derive(Debug, Clone)]
pub enum ResolveDriverErrorReason {
    Owned(String),
    Static(&'static str),
}

impl fmt::Display for ResolveDriverErrorReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveDriverErrorReason::Owned(s) => f.write_str(s),
            ResolveDriverErrorReason::Static(s) => f.write_str(s),
        }
    }
}

#[derive(Error, Debug, Clone)]
pub enum ResolveError {
    #[error("empty domain")]
    EmptyDomain,
    #[error("empty result")]
    EmptyResult,
    #[error("invalid redirection domain")]
    InvalidRedirectionDomain,
    #[error("no resolver set")]
    NoResolverSet,
    #[error("no resolver running")]
    NoResolverRunning,
    #[error("request timeout")]
    RequestTimeout,
    #[error("server error: {0}")]
    ServerError(#[from] ResolveServerError),
    #[error("driver error: {0}")]
    DriverError(ResolveDriverErrorReason),
    #[error("time out")]
    DriverTimeout,
}

impl ResolveError {
    pub fn get_type(&self) -> &str {
        match self {
            ResolveError::ServerError(_) => "ServerError",
            ResolveError::DriverError(_) => "DriverError",
            ResolveError::DriverTimeout => "DriverTimeout",
            _ => "LocalError",
        }
    }

    pub fn get_subtype(&self) -> &str {
        match self {
            ResolveError::ServerError(e) => e.get_type(),
            ResolveError::DriverError(_) | ResolveError::DriverTimeout => "",
            ResolveError::EmptyDomain => "EmptyDomain",
            ResolveError::EmptyResult => "EmptyResult",
            ResolveError::InvalidRedirectionDomain => "InvalidRedirectionDomain",
            ResolveError::NoResolverSet => "NoResolverSet",
            ResolveError::NoResolverRunning => "NoResolverRunning",
            ResolveError::RequestTimeout => "RequestTimeout",
        }
    }
}

impl From<ResolveDriverErrorReason> for ResolveError {
    fn from(value: ResolveDriverErrorReason) -> Self {
        ResolveError::DriverError(value)
    }
}
