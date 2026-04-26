/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fmt;
use std::str::FromStr;

use arcstr::ArcStr;
use idna::AsciiDenyList;
use thiserror::Error;

#[macro_export]
macro_rules! literal_domain {
    ($domain:literal) => {
        if $domain.ends_with('.') {
            $crate::net::DomainName::from_static($domain)
        } else {
            $crate::net::DomainName::from_static(concat!($domain, "."))
        }
    };
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct DomainName {
    fqdn: ArcStr,
}

impl DomainName {
    pub const MAX_LEN: usize = 254;
    pub const MAX_FQDN_LEN: usize = 255;

    pub fn from_static(s: &'static str) -> DomainName {
        DomainName::from_str(s).unwrap()
    }

    pub fn len_u8(&self) -> u8 {
        self.fqdn.len() as u8 - 1
    }

    pub fn as_str(&self) -> &str {
        &self.as_fqdn_str()[..self.fqdn.len() - 1]
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.fqdn.as_bytes()[..self.fqdn.len() - 1]
    }

    pub fn to_reversed(&self) -> String {
        let mut s = self
            .as_str()
            .split('.')
            .rev()
            .collect::<Vec<&str>>()
            .join(".");
        s.push('.');
        s
    }

    pub fn strip_suffix(&self, suffix: &DomainName) -> Option<&str> {
        let prefix = self.as_str().strip_suffix(suffix.as_str())?;
        prefix.strip_suffix('.')
    }

    pub fn replace_suffix(
        &self,
        from: &DomainName,
        to: &DomainName,
    ) -> Result<Option<DomainName>, DomainNameParseError> {
        let Some(s) = self.strip_suffix(from) else {
            return Ok(None);
        };

        let new_suffix = to.as_str();
        let new_len = s.len() + 1 + new_suffix.len() + 1;
        if new_len > DomainName::MAX_FQDN_LEN {
            return Err(DomainNameParseError::TooLong);
        }
        let mut new = String::with_capacity(new_len);
        new.push_str(s);
        new.push('.');
        new.push_str(to.as_str());
        new.push('.');
        Ok(Some(DomainName {
            fqdn: ArcStr::from(new),
        }))
    }

    pub fn as_fqdn_str(&self) -> &str {
        &self.fqdn
    }

    pub fn parse(b: &[u8]) -> Result<Self, DomainNameParseError> {
        let mut start = 0;
        // strip the '.' at the start
        while start < b.len() {
            if b[start] != b'.' {
                break;
            }
            start += 1;
        }
        let b = &b[start..];
        if b.is_empty() {
            return Err(DomainNameParseError::Empty);
        }

        // allow more than domain_to_ascii_strict chars
        let domain = idna::domain_to_ascii_cow(b, AsciiDenyList::EMPTY)?;
        if !domain.ends_with('.') {
            if domain.len() > DomainName::MAX_LEN {
                return Err(DomainNameParseError::TooLong);
            }
            let mut domain = domain.to_string();
            domain.push('.');
            Ok(DomainName {
                fqdn: ArcStr::from(domain),
            })
        } else if domain.len() > DomainName::MAX_FQDN_LEN {
            Err(DomainNameParseError::TooLong)
        } else {
            Ok(DomainName {
                fqdn: ArcStr::from(domain),
            })
        }
    }
}

impl fmt::Display for DomainName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Error)]
pub enum DomainNameParseError {
    #[error("empty domain")]
    Empty,
    #[error("too long domain")]
    TooLong,
    #[error("invalid domain: {0}")]
    Invalid(#[from] idna::Errors),
}

impl FromStr for DomainName {
    type Err = DomainNameParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        DomainName::parse(s.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ok() {
        let domain = DomainName::from_str("example.com").unwrap();
        assert_eq!(domain.as_str(), "example.com");
        assert_eq!(domain.as_bytes(), b"example.com");
        assert_eq!(domain.as_fqdn_str(), "example.com.");
        assert_eq!(domain.to_string(), "example.com");

        let domain = DomainName::from_str("example.com.").unwrap();
        assert_eq!(domain.as_str(), "example.com");
        assert_eq!(domain.as_bytes(), b"example.com");
        assert_eq!(domain.as_fqdn_str(), "example.com.");
        assert_eq!(domain.to_string(), "example.com");

        let domain = DomainName::from_str(".example.com").unwrap();
        assert_eq!(domain.as_str(), "example.com");
        assert_eq!(domain.as_bytes(), b"example.com");
        assert_eq!(domain.as_fqdn_str(), "example.com.");
        assert_eq!(domain.to_string(), "example.com");
    }

    #[test]
    fn parse_err() {
        let e = DomainName::from_str("").unwrap_err();
        assert!(matches!(e, DomainNameParseError::Empty));
    }

    #[test]
    fn reverse() {
        let domain = DomainName::from_str("example.com").unwrap();
        assert_eq!(domain.to_reversed(), "com.example.");
    }

    #[test]
    fn suffix() {
        let domain = DomainName::from_str("www.example.com").unwrap();
        let suffix1 = DomainName::from_str("example.com").unwrap();
        let suffix2 = DomainName::from_str("example.net").unwrap();

        let prefix = domain.strip_suffix(&suffix1).unwrap();
        assert_eq!(prefix, "www");

        let domain2 = domain.replace_suffix(&suffix1, &suffix2).unwrap().unwrap();
        assert_eq!(domain2.as_str(), "www.example.net");
    }
}
