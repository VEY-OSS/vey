/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod error;
pub use error::{HttpUpgradeError, HttpUpgradeResponseError};

mod request;
pub use request::HttpUpgradeRequest;

mod response;
pub use response::HttpUpgradeResponse;
