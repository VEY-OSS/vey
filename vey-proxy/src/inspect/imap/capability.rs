/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#[derive(Default)]
pub(super) struct Capability {
    imap4_rev1: bool,
    imap4_rev2: bool,
    has_non_sync_literal: bool,
    login_disabled: bool,
}

impl Capability {
    pub(super) fn check_supported<'a>(
        &mut self,
        cap: &'a str,
        from_enable: bool,
    ) -> Option<&'a str> {
        if let Some(p) = memchr::memchr(b'=', cap.as_bytes()) {
            let name = cap[..p].to_ascii_uppercase();
            match name.as_bytes() {
                b"AUTH" => {}
                b"CONTEXT" => {}            // rfc5267
                b"I18NLEVEL" => {}          // rfc5255
                b"STATUS" => {}             // rfc8438
                b"QUOTA" => {}              // rfc9208
                b"RIGHTS" => {}             // rfc4314
                b"APPENDLIMIT" => {}        // rfc7889
                b"COMPRESS" => return None, // rfc4978, no support
                b"IMAPSIEVE" => {}          // rfc6785
                b"SEARCH" => {}             // rfc6203
                b"SORT" => {}               // rfc5957
                b"URLAUTH" => {}            // rfc5524
                b"UTF8" => {}               // rfc6855
                _ => return None,
            }
        } else {
            let name = cap.to_ascii_uppercase();
            match name.as_bytes() {
                b"IMAP4" => {}
                b"IMAP4REV1" => self.imap4_rev1 = true,
                b"IMAP4REV2" => {
                    if from_enable {
                        self.imap4_rev2 = true;
                    }
                }
                b"LOGINDISABLED" => self.login_disabled = true,
                b"STARTTLS" => {}
                b"UIDPLUS" => {}   // rfc4315, rev2
                b"SASL-IR" => {}   // rfc4959, rev2
                b"MOVE" => {}      // rfc6851, rev2
                b"ID" => {}        // rfc2971, rev2
                b"UNSELECT" => {}  // rfc3691, rev2
                b"CHILDREN" => {}  // rfc3348, rev2
                b"IDLE" => {}      // rfc2177, rev2
                b"NAMESPACE" => {} // rfc2342, rev2
                b"ESEARCH" => {}
                b"SEARCHRES" => {}                           // rfc5182, rev2
                b"ENABLE" => {}                              // rfc5161, rev2
                b"LIST-EXTENDED" => {}                       // rfc5258, rev2
                b"LIST-STATUS" => {}                         // rfc5819, rev2
                b"CREATE-SPECIAL-USE" | b"SPECIAL-USE" => {} // rfc6154, rev2
                b"LITERAL+" if !self.has_non_sync_literal => {
                    // // rfc7888
                    self.has_non_sync_literal = true;
                }
                b"LITERAL-" if !self.has_non_sync_literal => {
                    // rfc7888, rev2
                    self.has_non_sync_literal = true;
                }
                b"BINARY" => {}                 // rfc3516, partially merged into rev2
                b"CONVERT" => {}                // rfc5259, require "BINARY"
                b"PARTIAL" => {}                // rfc9394
                b"ESORT" => {}                  // rfc5267
                b"SORT" | b"THREAD" => {}       // rfc5256
                b"LANGUAGE" => {}               // rfc5255
                b"MULTISEARCH" => {}            // rfc7377
                b"MULTIAPPEND" => {}            // rfc3502
                b"CONDSTORE" | b"QRESYNC" => {} // rfc7162
                b"QUOTA" | b"QUOTASET" => {}    // rfc9208
                b"ACL" => {}                    // rfc4314
                b"APPENDLIMIT" => {}            // rfc7889
                b"CATENATE" => return None,     // rfc4469, let's skip it
                b"URL-PARTIAL" => return None,  // rfc5550
                b"FILTERS" => {}                // rfc5466
                b"INPROGRESS" => {}             // rfc9585
                b"LIST-METADATA" => {}          // rfc9590
                b"LIST-MYRIGHTS" => {}          // rfc8440
                b"LOGIN-REFERRALS" => {}        // rfc2221
                b"MAILBOX-REFERRALS" => {}      // rfc2193
                b"METADATA" | b"METADATA-SERVER" => {} // rfc5464
                b"NOTIFY" => {}                 // rfc5465
                b"OBJECTID" => {}               // rfc8474
                b"PREVIEW" => {}                // rfc8970
                b"REPLACE" => {}                // rfc8508
                b"SAVEDATE" => {}               // rfc8514
                b"UIDONLY" => {}                // rfc9586
                b"UNAUTHENTICATE" => return None, // rfc8437, let's skip it
                b"URLAUTH" => {}                // rfc4467
                b"WITHIN" => {}                 // rfc5032
                _ => return None,
            }
        }

        Some(cap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_and_filtered() {
        let mut cap = Capability::default();
        assert_eq!(cap.check_supported("IMAP4rev1", false), Some("IMAP4rev1"));
        assert!(cap.imap4_rev1);
        assert_eq!(cap.check_supported("IMAP4REV1", false), Some("IMAP4REV1"));
        assert_eq!(cap.check_supported("imap4rev1", false), Some("imap4rev1"));
        assert_eq!(cap.check_supported("AUTH=PLAIN", false), Some("AUTH=PLAIN"));
        assert_eq!(cap.check_supported("auth=plain", false), Some("auth=plain"));
        assert_eq!(cap.check_supported("STARTTLS", false), Some("STARTTLS"));
        assert_eq!(cap.check_supported("COMPRESS=DEFLATE", false), None);
        assert_eq!(cap.check_supported("CATENATE", false), None);
        assert_eq!(cap.check_supported("UNAUTHENTICATE", false), None);
    }

    #[test]
    fn literal_plus_and_minus_are_exclusive() {
        let mut cap = Capability::default();
        assert_eq!(cap.check_supported("LITERAL+", false), Some("LITERAL+"));
        assert!(cap.has_non_sync_literal);
        assert_eq!(cap.check_supported("LITERAL-", false), None);

        let mut cap = Capability::default();
        assert_eq!(cap.check_supported("LITERAL-", false), Some("LITERAL-"));
        assert_eq!(cap.check_supported("LITERAL+", false), None);
    }

    #[test]
    fn imap4rev2_recorded_only_from_enable() {
        let mut cap = Capability::default();
        assert_eq!(cap.check_supported("IMAP4rev2", false), Some("IMAP4rev2"));
        assert!(!cap.imap4_rev2);
        assert_eq!(cap.check_supported("IMAP4rev2", true), Some("IMAP4rev2"));
        assert!(cap.imap4_rev2);
    }
}
