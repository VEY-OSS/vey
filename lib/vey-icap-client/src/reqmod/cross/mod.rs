/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

//! Cross-protocol REQMOD: H1 client ↔ H2 origin and H2 client ↔ H1 origin.
//!
//! Same-protocol adapters stay in [`super::h1`] / [`super::h2`]. Body framing:
//! - H2 origin always uses DATA + trailers (`Content-Length` is metadata only).
//! - H1 origin always uses chunked encoding so H2 trailers are not dropped.

mod convert;
pub use convert::{
    adapt_request_to_h2, serialize_adapted_request_as_h1_chunked,
    serialize_h2_request_as_h1_chunked,
};

mod error;
pub use error::{H1ToH2ReqmodAdaptationError, H2ToH1ReqmodAdaptationError};

pub mod h1_to_h2;
pub use h1_to_h2::{H1ToH2ReqmodEndState, H1ToH2ReqmodRunState, H1ToH2RequestAdapter};
