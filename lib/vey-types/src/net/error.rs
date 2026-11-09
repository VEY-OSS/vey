/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConnectError {
    #[error("connection refused")]
    ConnectionRefused,
    #[error("connection reset")]
    ConnectionReset,
    #[error("network unreachable")]
    NetworkUnreachable,
    #[error("host unreachable")] // from ICMP or local route
    HostUnreachable,
    #[error("timed out")]
    TimedOut,
    #[error("unspecified error: {0:?}")]
    UnspecifiedError(io::Error),
}

impl ConnectError {
    /// The remote host or its network answered by refusing, resetting, timing
    /// out, or being unreachable. Local faults such as a full port range stay out.
    pub fn is_remote_connect_failure(&self) -> bool {
        matches!(
            self,
            ConnectError::ConnectionRefused
                | ConnectError::ConnectionReset
                | ConnectError::HostUnreachable
                | ConnectError::TimedOut
        )
    }
}

impl From<io::Error> for ConnectError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::ConnectionRefused => return ConnectError::ConnectionRefused,
            io::ErrorKind::ConnectionReset => return ConnectError::ConnectionReset,
            io::ErrorKind::HostUnreachable => return ConnectError::HostUnreachable,
            io::ErrorKind::NetworkUnreachable => return ConnectError::NetworkUnreachable,
            io::ErrorKind::TimedOut => return ConnectError::TimedOut,
            _ => {}
        }
        if let Some(code) = e.raw_os_error() {
            match code {
                libc::ENETUNREACH => return ConnectError::NetworkUnreachable,
                libc::ECONNRESET => return ConnectError::ConnectionReset,
                libc::ETIMEDOUT => return ConnectError::TimedOut,
                libc::ECONNREFUSED => return ConnectError::ConnectionRefused,
                libc::EHOSTUNREACH => return ConnectError::HostUnreachable,
                _ => {}
            }
        }
        ConnectError::UnspecifiedError(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_io_error_kind() {
        let io_err = io::Error::new(io::ErrorKind::ConnectionRefused, "test");
        assert!(matches!(
            ConnectError::from(io_err),
            ConnectError::ConnectionRefused
        ));

        let io_err = io::Error::new(io::ErrorKind::ConnectionReset, "test");
        assert!(matches!(
            ConnectError::from(io_err),
            ConnectError::ConnectionReset
        ));

        let io_err = io::Error::new(io::ErrorKind::NetworkUnreachable, "test");
        assert!(matches!(
            ConnectError::from(io_err),
            ConnectError::NetworkUnreachable
        ));

        let io_err = io::Error::new(io::ErrorKind::HostUnreachable, "test");
        assert!(matches!(
            ConnectError::from(io_err),
            ConnectError::HostUnreachable
        ));

        let io_err = io::Error::new(io::ErrorKind::TimedOut, "test");
        assert!(matches!(ConnectError::from(io_err), ConnectError::TimedOut));

        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "test");
        assert!(matches!(
            ConnectError::from(io_err),
            ConnectError::UnspecifiedError(_)
        ));
    }

    #[test]
    fn from_raw_os_error() {
        let io_err = io::Error::from_raw_os_error(libc::ENETUNREACH);
        assert!(matches!(
            ConnectError::from(io_err),
            ConnectError::NetworkUnreachable
        ));

        let io_err = io::Error::from_raw_os_error(libc::ECONNRESET);
        assert!(matches!(
            ConnectError::from(io_err),
            ConnectError::ConnectionReset
        ));

        let io_err = io::Error::from_raw_os_error(libc::ETIMEDOUT);
        assert!(matches!(ConnectError::from(io_err), ConnectError::TimedOut));

        let io_err = io::Error::from_raw_os_error(libc::ECONNREFUSED);
        assert!(matches!(
            ConnectError::from(io_err),
            ConnectError::ConnectionRefused
        ));

        let io_err = io::Error::from_raw_os_error(libc::EHOSTUNREACH);
        assert!(matches!(
            ConnectError::from(io_err),
            ConnectError::HostUnreachable
        ));

        let io_err = io::Error::from_raw_os_error(libc::EACCES);
        assert!(matches!(
            ConnectError::from(io_err),
            ConnectError::UnspecifiedError(_)
        ));
    }

    #[test]
    fn remote_connect_failure_excludes_local_errors() {
        assert!(ConnectError::ConnectionRefused.is_remote_connect_failure());
        assert!(ConnectError::ConnectionReset.is_remote_connect_failure());
        assert!(ConnectError::HostUnreachable.is_remote_connect_failure());
        assert!(ConnectError::TimedOut.is_remote_connect_failure());
        assert!(!ConnectError::NetworkUnreachable.is_remote_connect_failure());
        assert!(
            !ConnectError::UnspecifiedError(io::Error::from_raw_os_error(libc::EMFILE))
                .is_remote_connect_failure()
        );
    }
}
