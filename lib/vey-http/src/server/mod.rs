/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

mod error;
pub use error::HttpRequestParseError;

mod request;
pub use request::HttpProxyClientRequest;

mod converted;
pub use converted::HttpConvertedRequest;

mod transparent;
pub use transparent::{HttpTransparentRequest, HttpTransparentRequestAcceptor};

mod adaptation;
pub use adaptation::HttpAdaptedRequest;

mod uri;
pub use uri::UriExt;
