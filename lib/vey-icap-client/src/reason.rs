/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fmt;

#[derive(Debug)]
pub enum IcapErrorReason {
    InvalidResponse,
    UnknownResponse,
    InvalidResponseAfterContinue,
    UnknownResponseAfterContinue,
    ContinueAfterPreviewEof,
    UnknownResponseForPreview,
    NoBodyFound,
}

impl IcapErrorReason {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            IcapErrorReason::InvalidResponse => "invalid ICAP response code",
            IcapErrorReason::UnknownResponse => "unknown ICAP response code",
            IcapErrorReason::InvalidResponseAfterContinue => {
                "invalid ICAP response code after 100-continue"
            }
            IcapErrorReason::UnknownResponseAfterContinue => {
                "unknown ICAP response code after 100-continue"
            }
            IcapErrorReason::ContinueAfterPreviewEof => {
                "invalid 100-continue response as preview is eof"
            }
            IcapErrorReason::UnknownResponseForPreview => "unknown ICAP response code for preview",
            IcapErrorReason::NoBodyFound => "no ICAP body found",
        }
    }
}

impl fmt::Display for IcapErrorReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_matches_as_str() {
        let reason = IcapErrorReason::NoBodyFound;
        assert_eq!(reason.to_string(), reason.as_str());
        assert_eq!(reason.to_string(), "no ICAP body found");
    }

    #[test]
    fn all_variants_have_nonempty_display() {
        let variants = [
            IcapErrorReason::InvalidResponse,
            IcapErrorReason::UnknownResponse,
            IcapErrorReason::InvalidResponseAfterContinue,
            IcapErrorReason::UnknownResponseAfterContinue,
            IcapErrorReason::ContinueAfterPreviewEof,
            IcapErrorReason::UnknownResponseForPreview,
            IcapErrorReason::NoBodyFound,
        ];
        for reason in variants {
            assert_eq!(reason.to_string(), reason.as_str());
            assert!(!reason.to_string().is_empty());
        }
    }
}
