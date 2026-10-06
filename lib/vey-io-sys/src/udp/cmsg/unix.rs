/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 */

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::{mem, ptr};

use super::{RecvAncillaryBuffer, RecvAncillaryData, SendAncillaryBuffer};
#[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd"))]
use crate::RawSocketAddr;

const fn cmsg_len(length: usize) -> usize {
    unsafe { libc::CMSG_LEN(length as _) as usize }
}

const fn cmsg_space(length: usize) -> usize {
    unsafe { libc::CMSG_SPACE(length as _) as usize }
}

const CMSG_HDR_SIZE: usize = cmsg_len(0);

impl RecvAncillaryBuffer {
    pub fn parse_msg<T: RecvAncillaryData>(
        &self,
        msghdr: libc::msghdr,
        data: &mut T,
    ) -> io::Result<()> {
        self.parse(msghdr.msg_controllen as _, data)
    }

    #[allow(clippy::single_match)]
    pub fn parse_buf<T: RecvAncillaryData>(control_buf: &[u8], data: &mut T) -> io::Result<()> {
        let total_size = control_buf.len();
        let mut offset = 0usize;

        while offset + CMSG_HDR_SIZE <= total_size {
            let buf = &control_buf[offset..];
            let hdr = unsafe { buf.as_ptr().cast::<libc::cmsghdr>().as_ref().unwrap() };
            let msg_len: usize = hdr.cmsg_len as _;
            if msg_len <= CMSG_HDR_SIZE {
                // empty record
                break;
            }
            if offset + msg_len > total_size {
                // too much payload data
                break;
            }
            offset += cmsg_space(msg_len - CMSG_HDR_SIZE);

            let payload = &buf[CMSG_HDR_SIZE..msg_len];

            match hdr.cmsg_level {
                libc::SOL_SOCKET => {}
                libc::IPPROTO_IP => match hdr.cmsg_type {
                    #[cfg(any(target_os = "linux", target_os = "android"))]
                    libc::IP_PKTINFO => {
                        if payload.len() < size_of::<libc::in_pktinfo>() {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "no enough msg data for struct in_pktinfo",
                            ));
                        }
                        let pktinfo = unsafe {
                            payload
                                .as_ptr()
                                .cast::<libc::in_pktinfo>()
                                .as_ref()
                                .unwrap()
                        };

                        let ifindex = u32::try_from(pktinfo.ipi_ifindex).unwrap_or_default();
                        data.set_recv_interface(ifindex);
                        let ip4 = Ipv4Addr::from(u32::from_be(pktinfo.ipi_addr.s_addr));
                        data.set_recv_dst_addr(IpAddr::V4(ip4));
                    }
                    #[cfg(not(any(
                        target_os = "linux",
                        target_os = "android",
                        target_os = "freebsd",
                        target_os = "openbsd",
                        target_os = "dragonfly"
                    )))]
                    libc::IP_PKTINFO => {
                        if payload.len() < size_of::<libc::in_pktinfo>() {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "no enough msg data for struct in_pktinfo",
                            ));
                        }
                        let pktinfo = unsafe {
                            payload
                                .as_ptr()
                                .cast::<libc::in_pktinfo>()
                                .as_ref()
                                .unwrap()
                        };

                        data.set_recv_interface(pktinfo.ipi_ifindex);
                        let ip4 = Ipv4Addr::from(u32::from_be(pktinfo.ipi_addr.s_addr));
                        data.set_recv_dst_addr(IpAddr::V4(ip4));
                    }
                    #[cfg(any(
                        target_os = "freebsd",
                        target_os = "openbsd",
                        target_os = "dragonfly"
                    ))]
                    libc::IP_RECVIF => {
                        if payload.len() < size_of::<libc::sockaddr_dl>() {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "no enough msg data for struct sockaddr_dl",
                            ));
                        }
                        let dl_addr = unsafe {
                            payload
                                .as_ptr()
                                .cast::<libc::sockaddr_dl>()
                                .as_ref()
                                .unwrap()
                        };
                        data.set_recv_interface(dl_addr.sdl_index as u32);
                    }
                    #[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd"))]
                    libc::IP_ORIGDSTADDR => {
                        let Some(addr) = RawSocketAddr::from_bytes(payload) else {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "invalid IP_ORIGDSTADDR",
                            ));
                        };
                        data.set_recv_orig_dst_addr(addr);
                    }
                    #[cfg(any(
                        target_os = "freebsd",
                        target_os = "openbsd",
                        target_os = "dragonfly"
                    ))]
                    libc::IP_RECVDSTADDR => {
                        if payload.len() < size_of::<libc::in_addr>() {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "no enough msg data for struct in_addr",
                            ));
                        }
                        let ipaddr =
                            unsafe { payload.as_ptr().cast::<libc::in_addr>().as_ref().unwrap() };
                        let ip4 = Ipv4Addr::from(u32::from_be(ipaddr.s_addr));
                        data.set_recv_dst_addr(IpAddr::V4(ip4));
                    }
                    _ => {}
                },
                libc::IPPROTO_IPV6 => match hdr.cmsg_type {
                    libc::IPV6_PKTINFO => {
                        if payload.len() < size_of::<libc::in6_pktinfo>() {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "no enough msg data for struct in6_pktinfo",
                            ));
                        }
                        let pktinfo = unsafe {
                            payload
                                .as_ptr()
                                .cast::<libc::in6_pktinfo>()
                                .as_ref()
                                .unwrap()
                        };

                        data.set_recv_interface(pktinfo.ipi6_ifindex);
                        let ip6 = Ipv6Addr::from(pktinfo.ipi6_addr.s6_addr);
                        data.set_recv_dst_addr(IpAddr::V6(ip6));
                    }
                    #[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd"))]
                    libc::IPV6_ORIGDSTADDR => {
                        let Some(addr) = RawSocketAddr::from_bytes(payload) else {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "invalid IPV6_ORIGDSTADDR",
                            ));
                        };
                        data.set_recv_orig_dst_addr(addr);
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        Ok(())
    }
}

impl SendAncillaryBuffer {
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "solaris",
        target_os = "illumos",
    ))]
    pub(super) fn encode_v4(&mut self, ip: Ipv4Addr) -> Option<()> {
        let mut info = unsafe { mem::zeroed::<libc::in_pktinfo>() };
        info.ipi_spec_dst.s_addr = u32::from(ip).to_be();
        self.write(
            libc::IPPROTO_IP,
            libc::IP_PKTINFO,
            ptr::from_ref(&info).cast(),
            size_of::<libc::in_pktinfo>(),
        )
    }

    #[cfg(any(
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd",
    ))]
    pub(super) fn encode_v4(&mut self, ip: Ipv4Addr) -> Option<()> {
        let mut addr = unsafe { mem::zeroed::<libc::in_addr>() };
        addr.s_addr = u32::from(ip).to_be();
        self.write(
            libc::IPPROTO_IP,
            libc::IP_SENDSRCADDR,
            ptr::from_ref(&addr).cast(),
            size_of::<libc::in_addr>(),
        )
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "solaris",
        target_os = "illumos",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd",
    )))]
    pub(super) fn encode_v4(&mut self, ip: Ipv4Addr) -> Option<()> {
        let _ = ip;
        None
    }

    pub(super) fn encode_v6(&mut self, ip: Ipv6Addr) -> Option<()> {
        let mut info = unsafe { mem::zeroed::<libc::in6_pktinfo>() };
        info.ipi6_addr.s6_addr = ip.octets();
        self.write(
            libc::IPPROTO_IPV6,
            libc::IPV6_PKTINFO,
            ptr::from_ref(&info).cast(),
            size_of::<libc::in6_pktinfo>(),
        )
    }

    fn write(
        &mut self,
        level: libc::c_int,
        cmsg_type: libc::c_int,
        data: *const u8,
        data_len: usize,
    ) -> Option<()> {
        let space = cmsg_space(data_len);
        let msg_len = cmsg_len(data_len);
        let offset = self.len;
        let end = offset.checked_add(space)?;
        if end > self.buf.len() {
            return None;
        }
        self.buf[offset..end].fill(0);
        unsafe {
            let hdr = self.buf.as_mut_ptr().add(offset).cast::<libc::cmsghdr>();
            (*hdr).cmsg_len = msg_len as _;
            (*hdr).cmsg_level = level;
            (*hdr).cmsg_type = cmsg_type;
            ptr::copy_nonoverlapping(data, libc::CMSG_DATA(hdr), data_len);
        }
        self.len = end;
        Some(())
    }
}

#[cfg(all(
    test,
    any(target_os = "linux", target_os = "android", target_os = "freebsd")
))]
mod tests {
    use std::io::IoSliceMut;
    use std::mem::size_of;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
    use std::os::fd::AsRawFd;

    use super::RecvAncillaryBuffer;
    use crate::RawSocketAddr;
    use crate::udp::RecvMsgHdr;

    fn cmsg_buf(level: libc::c_int, cmsg_type: libc::c_int, payload: &[u8]) -> Vec<u8> {
        let msg_len = unsafe { libc::CMSG_LEN(payload.len() as _) as usize };
        let space = unsafe { libc::CMSG_SPACE(payload.len() as _) as usize };
        let mut buf = vec![0u8; space];
        unsafe {
            let hdr = buf.as_mut_ptr().cast::<libc::cmsghdr>();
            (*hdr).cmsg_len = msg_len as _;
            (*hdr).cmsg_level = level;
            (*hdr).cmsg_type = cmsg_type;
        }
        let data_off = unsafe { libc::CMSG_LEN(0) as usize };
        buf[data_off..data_off + payload.len()].copy_from_slice(payload);
        buf
    }

    fn sockaddr_bytes(addr: SocketAddr) -> Vec<u8> {
        RawSocketAddr::from(addr).as_bytes().to_vec()
    }

    fn parse_dst(buf: &[u8], listen: SocketAddr) -> SocketAddr {
        let mut payload = [0u8; 8];
        let mut hdr = RecvMsgHdr::new([IoSliceMut::new(&mut payload)]);
        RecvAncillaryBuffer::parse_buf(buf, &mut hdr).unwrap();
        hdr.dst_addr(listen)
    }

    #[test]
    fn origdstaddr_keeps_port_distinct_from_listen_port() {
        let orig = SocketAddr::from((Ipv4Addr::new(1, 2, 3, 4), 53));
        let listen = SocketAddr::from((Ipv4Addr::LOCALHOST, 8123));
        let buf = cmsg_buf(
            libc::IPPROTO_IP,
            libc::IP_ORIGDSTADDR,
            &sockaddr_bytes(orig),
        );
        assert_eq!(parse_dst(&buf, listen), orig);
    }

    #[test]
    fn ipv6_origdstaddr_keeps_full_address() {
        let orig = SocketAddr::from((Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1), 443));
        let listen = SocketAddr::from((Ipv6Addr::LOCALHOST, 8123));
        let buf = cmsg_buf(
            libc::IPPROTO_IPV6,
            libc::IPV6_ORIGDSTADDR,
            &sockaddr_bytes(orig),
        );
        assert_eq!(parse_dst(&buf, listen), orig);
    }

    #[test]
    fn recvmsg_reports_original_ipv4_destination() {
        let listener = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
        let enable = 1 as libc::c_int;
        let rc = unsafe {
            libc::setsockopt(
                listener.as_raw_fd(),
                libc::IPPROTO_IP,
                libc::IP_RECVORIGDSTADDR,
                std::ptr::from_ref(&enable).cast(),
                size_of::<libc::c_int>() as _,
            )
        };
        assert_eq!(rc, 0, "{}", std::io::Error::last_os_error());

        let bound = listener.local_addr().unwrap();
        let target = SocketAddr::from((Ipv4Addr::LOCALHOST, bound.port()));
        let client = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        client.send_to(b"ping", target).unwrap();

        let mut bytes = [0u8; 16];
        let mut hdr = RecvMsgHdr::new([IoSliceMut::new(&mut bytes)]);
        let mut control = RecvAncillaryBuffer::new();
        let mut msg = unsafe { hdr.to_msghdr(&mut control) };
        let n = crate::udp::recvmsg(&listener, &mut msg).unwrap();
        hdr.n_recv = n;
        control.parse(msg.msg_controllen as _, &mut hdr).unwrap();

        assert_eq!(hdr.dst_addr(bound), target);
    }
}
