/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

mod auth;
pub use auth::{
    is_session_based_auth, proxy_authenticate_basic, proxy_authorization_basic,
    www_authenticate_basic,
};

mod connection;
pub use connection::{connection_as_bytes, remove_h2_connection_specific_headers};

mod content;
pub use content::{content_length, content_range_overflowed, content_range_sized, content_type};

mod transfer;
pub use transfer::transfer_encoding_chunked;
