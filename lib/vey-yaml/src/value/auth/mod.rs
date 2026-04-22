/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 VEY-OSS Developers.
 */

mod basic;
pub use basic::{as_password, as_username};

#[cfg(feature = "auth-facts")]
mod facts;
#[cfg(feature = "auth-facts")]
pub use facts::{as_facts_match_type, as_facts_match_value};
