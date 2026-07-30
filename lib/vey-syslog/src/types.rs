/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

#[allow(unused)]
#[derive(Copy, Clone, Debug)]
pub enum Facility {
    // kernel messages (these can't be generated from user processes)
    Kern = 0 << 3,
    // generic user-level messages
    User = 1 << 3,
    // mail subsystem
    Mail = 2 << 3,
    // system daemons without separate facility value
    Daemon = 3 << 3,
    // security/authorization messages
    Auth = 4 << 3,
    // messages generated internally by syslogd(8)
    Syslog = 5 << 3,
    // line printer subsystem
    Lpr = 6 << 3,
    // USENET news subsystem
    News = 7 << 3,
    // UUCP subsystem
    Uucp = 8 << 3,
    // clock daemon (cron and at)
    Cron = 9 << 3,
    // security/authorization messages (private)
    AuthPrivate = 10 << 3,
    // ftp daemon
    Ftp = 11 << 3,
    Local0 = 16 << 3,
    Local1 = 17 << 3,
    Local2 = 18 << 3,
    Local3 = 19 << 3,
    Local4 = 20 << 3,
    Local5 = 21 << 3,
    Local6 = 22 << 3,
    Local7 = 23 << 3,
}

#[allow(unused)]
#[derive(Copy, Clone)]
pub enum Severity {
    // system is unusable
    Emergency,
    // action must be taken immediately
    Alert,
    // critical conditions
    Critical,
    // error conditions
    Error,
    // warning conditions
    Warning,
    // normal, but significant, condition
    Notice,
    // informational message
    Info,
    // debug-level message
    Debug,
}

pub type Priority = u8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facility_values_are_multiples_of_eight() {
        assert_eq!(Facility::Kern as u8, 0);
        assert_eq!(Facility::User as u8, 8);
        assert_eq!(Facility::Mail as u8, 16);
        assert_eq!(Facility::Local7 as u8, 23 << 3);
    }

    #[test]
    fn severity_values_are_sequential() {
        assert_eq!(Severity::Emergency as u8, 0);
        assert_eq!(Severity::Alert as u8, 1);
        assert_eq!(Severity::Debug as u8, 7);
    }

    #[test]
    fn facility_covers_standard_and_local_range() {
        assert_eq!(Facility::Auth as u8, 4 << 3);
        assert_eq!(Facility::Syslog as u8, 5 << 3);
        assert_eq!(Facility::Cron as u8, 9 << 3);
        assert_eq!(Facility::Ftp as u8, 11 << 3);
        assert_eq!(Facility::Local0 as u8, 16 << 3);
        assert_eq!(Facility::Local3 as u8, 19 << 3);
        assert_eq!(Facility::Local6 as u8, 22 << 3);
    }
}
