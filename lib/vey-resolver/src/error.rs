/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
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
pub enum ResolveLocalError {
    #[error("no resolver set")]
    NoResolverSet,
    #[error("no resolver running")]
    NoResolverRunning,
    #[error("driver timed out")]
    DriverTimedOut,
}

impl ResolveLocalError {
    pub fn get_type(&self) -> &str {
        match self {
            ResolveLocalError::NoResolverSet => "NoResolverSet",
            ResolveLocalError::NoResolverRunning => "NoResolverRunning",
            ResolveLocalError::DriverTimedOut => "DriverTimedOut",
        }
    }
}

#[derive(Error, Debug, Clone)]
pub enum ResolveError {
    #[error("empty domain")]
    EmptyDomain,
    #[error("empty result")]
    EmptyResult,
    #[error("server error: {0}")]
    ServerError(#[from] ResolveServerError),
    #[error("driver error: {0}")]
    DriverError(ResolveDriverErrorReason),
    #[error("time out")]
    DriverTimeout,
    #[error("local error: {0}")]
    FromLocal(#[from] ResolveLocalError),
    #[error("unexpected error: {0}")]
    UnexpectedError(&'static str),
}

impl ResolveError {
    pub fn get_type(&self) -> &str {
        match self {
            ResolveError::EmptyDomain => "EmptyDomain",
            ResolveError::EmptyResult => "EmptyResult",
            ResolveError::ServerError(_) => "ServerError",
            ResolveError::DriverError(_) => "DriverError",
            ResolveError::DriverTimeout => "DriverTimeout",
            ResolveError::FromLocal(_) => "LocalError",
            ResolveError::UnexpectedError(_) => "UnexpectedError",
        }
    }

    pub fn get_subtype(&self) -> &str {
        match self {
            ResolveError::EmptyDomain | ResolveError::EmptyResult => "",
            ResolveError::ServerError(e) => e.get_type(),
            ResolveError::DriverError(_) => "",
            ResolveError::DriverTimeout => "",
            ResolveError::FromLocal(e) => e.get_type(),
            ResolveError::UnexpectedError(_) => "",
        }
    }
}

impl From<ResolveDriverErrorReason> for ResolveError {
    fn from(value: ResolveDriverErrorReason) -> Self {
        ResolveError::DriverError(value)
    }
}
