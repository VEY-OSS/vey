/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use vey_types::log::AsyncLogConfig;

mod async_streamer;

mod backend;
mod format;
mod types;
mod util;

#[cfg(feature = "yaml")]
mod yaml;

pub use types::{Facility, Severity};

use async_streamer::AsyncSyslogStreamer;

pub use backend::SyslogBackendBuilder;

use format::BoxSyslogFormatter;
pub use format::SyslogFormatterKind;

pub struct SyslogHeader {
    pub facility: Facility,
    pub hostname: Option<String>,
    pub process: String,
    pub pid: u32,
}

#[derive(Clone, Debug)]
pub struct SyslogBuilder {
    ident: String,
    facility: Facility,
    backend: SyslogBackendBuilder,
    format: SyslogFormatterKind,
    emit_hostname: bool,
    append_report_ts: bool,
}

impl SyslogBuilder {
    pub fn with_ident(ident: &str) -> Self {
        SyslogBuilder {
            ident: String::from(ident),
            facility: Facility::User,
            backend: SyslogBackendBuilder::default(),
            format: SyslogFormatterKind::Rfc3164,
            emit_hostname: false,
            append_report_ts: false,
        }
    }

    pub fn set_facility(&mut self, facility: Facility) {
        self.facility = facility;
    }

    pub fn set_backend(&mut self, kind: SyslogBackendBuilder) {
        self.backend = kind;
    }

    pub fn set_format(&mut self, kind: SyslogFormatterKind) {
        self.format = kind;
    }

    pub fn enable_cee_log_syntax(&mut self, event_flag: Option<String>) {
        let event_flag = event_flag.unwrap_or_else(|| format::CEE_EVENT_FLAG.to_owned());
        self.format = match &self.format {
            SyslogFormatterKind::Rfc3164 | SyslogFormatterKind::Rfc3164Cee(_) => {
                SyslogFormatterKind::Rfc3164Cee(event_flag)
            }
            SyslogFormatterKind::Rfc5424(_, mid) | SyslogFormatterKind::Rfc5424Cee(mid, _) => {
                SyslogFormatterKind::Rfc5424Cee(mid.clone(), event_flag)
            }
        };
    }

    pub fn set_emit_hostname(&mut self, enable: bool) {
        self.emit_hostname = enable;
    }

    pub fn append_report_ts(&mut self, enable: bool) {
        self.append_report_ts = enable;
    }

    pub fn start_async(self, async_conf: &AsyncLogConfig) -> AsyncSyslogStreamer {
        let hostname = if self.emit_hostname {
            Some(vey_compat::hostname().to_string_lossy().into_owned())
        } else {
            None
        };

        let header = SyslogHeader {
            facility: self.facility,
            hostname,
            process: self.ident,
            pid: std::process::id(),
        };

        let mut formatter = match self.format {
            SyslogFormatterKind::Rfc3164 => {
                let formatter = format::FormatterRfc3164::new();
                Box::new(formatter) as BoxSyslogFormatter
            }
            SyslogFormatterKind::Rfc3164Cee(event_flag) => {
                let formatter = format::FormatterRfc3164Cee::new(event_flag);
                Box::new(formatter) as BoxSyslogFormatter
            }
            SyslogFormatterKind::Rfc5424(eid, mid) => {
                let formatter = format::FormatterRfc5424::new(eid, mid);
                Box::new(formatter) as BoxSyslogFormatter
            }
            SyslogFormatterKind::Rfc5424Cee(mid, event_flag) => {
                let formatter = format::FormatterRfc5424Cee::new(mid, event_flag);
                Box::new(formatter) as BoxSyslogFormatter
            }
        };
        formatter.append_report_ts(self.append_report_ts);
        AsyncSyslogStreamer::new(async_conf, header, formatter, &self.backend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn builder_defaults_and_setters() {
        let mut builder = SyslogBuilder::with_ident("vey");
        assert_eq!(builder.ident, "vey");
        assert!(matches!(builder.facility, Facility::User));
        assert!(matches!(builder.format, SyslogFormatterKind::Rfc3164));
        assert!(!builder.emit_hostname);
        assert!(!builder.append_report_ts);

        builder.set_facility(Facility::Local0);
        builder.set_format(SyslogFormatterKind::Rfc5424(32473, Some("MID".into())));
        builder.set_emit_hostname(true);
        builder.append_report_ts(true);
        builder.set_backend(SyslogBackendBuilder::Udp(
            None,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 514),
        ));

        assert!(matches!(builder.facility, Facility::Local0));
        assert!(builder.emit_hostname);
        assert!(builder.append_report_ts);
        match &builder.format {
            SyslogFormatterKind::Rfc5424(eid, mid) => {
                assert_eq!(*eid, 32473);
                assert_eq!(mid.as_deref(), Some("MID"));
            }
            _ => panic!("expected Rfc5424"),
        }
        assert!(matches!(builder.backend, SyslogBackendBuilder::Udp(_, _)));
    }

    #[test]
    fn enable_cee_log_syntax_from_rfc3164_and_rfc5424() {
        let mut builder = SyslogBuilder::with_ident("vey");
        builder.enable_cee_log_syntax(None);
        match &builder.format {
            SyslogFormatterKind::Rfc3164Cee(flag) => assert_eq!(flag, "@cee:"),
            _ => panic!("expected Rfc3164Cee"),
        }

        builder.set_format(SyslogFormatterKind::Rfc5424(1, Some("ID".into())));
        builder.enable_cee_log_syntax(Some("FLAG:".into()));
        match &builder.format {
            SyslogFormatterKind::Rfc5424Cee(mid, flag) => {
                assert_eq!(mid.as_deref(), Some("ID"));
                assert_eq!(flag, "FLAG:");
            }
            _ => panic!("expected Rfc5424Cee"),
        }
    }
}
