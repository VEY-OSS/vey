/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod map;
pub use map::HttpHeaderMap;

mod name;
pub use name::{HttpKnownHeader, HttpKnownHeaderName, HttpOriginalHeaderName};

mod value;
pub use value::HttpHeaderValue;

mod server_id;
pub use server_id::HttpServerId;

mod forwarded;
pub use forwarded::{
    HttpForwardedHeaderType, HttpForwardedHeaderValue, HttpStandardForwardedHeaderValue,
};

mod transfer;
pub use transfer::{
    AcceptTransferEncodingValue, InvalidAcceptTransferEncodingValue, InvalidTransferEncodingValue,
    TransferCodingQValue, TransferCompressKind, TransferEncodingValue,
};

mod connection;
pub use connection::{ConnectionValue, KeepAliveValue};

pub mod http_names;

mod item_list;
use item_list::GenericItem;

/// Parser for common HTTP header field values.
trait HttpFieldParser {
    /// Split a common comma-separated header value into [`GenericItem`]s.
    ///
    /// Members are split on `,`, then an optional param suffix on the first `;`.
    /// OWS is trimmed and empty members are skipped. Quoted strings, escapes, and
    /// inner lists are not recognized.
    fn as_generic_item_list(&self) -> impl Iterator<Item = GenericItem<'_>>;
}
