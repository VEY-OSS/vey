/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

//! Cross-protocol RESPMOD: H1 origin ↔ H2 client and H2 origin ↔ H1 client.
//!
//! Same-protocol adapters stay in [`super::h1`] / [`super::h2`]. Body framing:
//! - H2 client always uses DATA + trailers (`Content-Length` is metadata only).
//! - H1 client always uses chunked encoding so H2 trailers are not dropped.

mod convert;
pub use convert::{
    adapt_response_to_h2, serialize_adapted_response_as_h1_chunked,
    serialize_h2_response_as_h1_chunked,
};

mod error;
pub use error::{H1ToH2RespmodAdaptationError, H2ToH1RespmodAdaptationError};

pub mod h1_to_h2;
pub mod h2_to_h1;

pub use h1_to_h2::{H1ToH2RespmodEndState, H1ToH2RespmodRunState, H1ToH2ResponseAdapter};
pub use h2_to_h1::{H2ToH1RespmodEndState, H2ToH1RespmodRunState, H2ToH1ResponseAdapter};
