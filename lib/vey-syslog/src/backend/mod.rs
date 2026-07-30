/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::net::{IpAddr, SocketAddr, UdpSocket};
#[cfg(unix)]
use std::os::unix::net::UnixDatagram;
#[cfg(unix)]
use std::path::PathBuf;

use bytes::Bytes;

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "solaris",
    target_os = "macos",
))]
use vey_io_sys::udp::UdpSocketExt;

#[cfg(feature = "yaml")]
mod yaml;

mod udp;
#[cfg(unix)]
mod unix_datagram;

pub(super) const MAX_BATCH_SIZE: usize = 32;

pub(super) enum SyslogBackend {
    Udp(UdpSocket),
    #[cfg(unix)]
    Unix(UnixDatagram),
}

impl SyslogBackend {
    pub(super) fn write_many(&self, msgs: &[Bytes]) -> io::Result<usize> {
        if msgs.len() == 1 {
            match self {
                SyslogBackend::Udp(s) => {
                    s.send(&msgs[0])?;
                }
                #[cfg(unix)]
                SyslogBackend::Unix(s) => {
                    s.send(&msgs[0])?;
                }
            }
            Ok(1)
        } else {
            match self {
                #[cfg(any(
                    target_os = "linux",
                    target_os = "android",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd",
                    target_os = "solaris",
                ))]
                SyslogBackend::Udp(s) => {
                    use vey_io_sys::udp::SendMsgHdr;

                    let mut hdrs: [SendMsgHdr<'_, 1>; MAX_BATCH_SIZE] =
                        unsafe { std::mem::zeroed() };
                    for (i, m) in msgs.iter().take(MAX_BATCH_SIZE).enumerate() {
                        hdrs[i] = SendMsgHdr::new([io::IoSlice::new(m)], None);
                    }
                    s.batch_sendmsg(&mut hdrs[..msgs.len().min(MAX_BATCH_SIZE)])
                }
                #[cfg(target_os = "macos")]
                SyslogBackend::Udp(s) => {
                    use vey_io_sys::udp::SendMsgHdr;

                    let mut hdrs: [SendMsgHdr<'_, 1>; MAX_BATCH_SIZE] =
                        unsafe { std::mem::zeroed() };
                    for (i, m) in msgs.iter().take(MAX_BATCH_SIZE).enumerate() {
                        hdrs[i] = SendMsgHdr::new([io::IoSlice::new(m)], None);
                    }
                    s.batch_sendmsg_x(&mut hdrs[..msgs.len().min(MAX_BATCH_SIZE)])
                }
                #[cfg(not(any(
                    target_os = "linux",
                    target_os = "android",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd",
                    target_os = "solaris",
                    target_os = "macos",
                )))]
                SyslogBackend::Udp(s) => {
                    s.send(&msgs[0])?;
                    Ok(1)
                }
                #[cfg(unix)]
                SyslogBackend::Unix(s) => {
                    s.send(&msgs[0])?;
                    Ok(1)
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum SyslogBackendBuilder {
    /// unix socket with path
    #[cfg(unix)]
    Unix(Option<PathBuf>),
    /// udp socket with optional bind ip and remote address
    Udp(Option<IpAddr>, SocketAddr),
}

#[cfg(unix)]
impl Default for SyslogBackendBuilder {
    fn default() -> Self {
        SyslogBackendBuilder::Unix(None)
    }
}

#[cfg(not(unix))]
impl Default for SyslogBackendBuilder {
    fn default() -> Self {
        use std::net::Ipv4Addr;

        SyslogBackendBuilder::Udp(None, SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 514))
    }
}

impl SyslogBackendBuilder {
    pub(super) fn build(&self) -> io::Result<SyslogBackend> {
        match self {
            #[cfg(unix)]
            SyslogBackendBuilder::Unix(path) => {
                let socket = if let Some(path) = path {
                    unix_datagram::custom(path)?
                } else {
                    unix_datagram::default()?
                };
                Ok(SyslogBackend::Unix(socket))
            }
            SyslogBackendBuilder::Udp(bind_ip, server) => {
                let socket = udp::udp(*bind_ip, *server)?;
                Ok(SyslogBackend::Udp(socket))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn udp_backend_builds_and_sends_to_connected_peer() {
        let server = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).unwrap();
        let addr = server.local_addr().unwrap();

        let backend = SyslogBackendBuilder::Udp(Some(IpAddr::V4(Ipv4Addr::LOCALHOST)), addr)
            .build()
            .unwrap();

        let n = backend
            .write_many(&[Bytes::from_static(b"hello-syslog")])
            .unwrap();
        assert_eq!(n, 1);

        let mut buf = [0u8; 32];
        let (nr, _) = server.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..nr], b"hello-syslog");
    }

    #[test]
    fn udp_backend_default_bind_accepts_ipv6_unspecified() {
        // Bind an IPv6 localhost socket when available; otherwise skip via dual-stack IPv4.
        let server = match UdpSocket::bind(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 0)) {
            Ok(s) => s,
            Err(_) => return,
        };
        let addr = server.local_addr().unwrap();
        assert!(SyslogBackendBuilder::Udp(None, addr).build().is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn default_backend_is_unix() {
        assert!(matches!(
            SyslogBackendBuilder::default(),
            SyslogBackendBuilder::Unix(None)
        ));
    }
}
