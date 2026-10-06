/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 */

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::{io, mem};

use windows_sys::Win32::Networking::WinSock;

use super::{RecvAncillaryBuffer, RecvAncillaryData, SendAncillaryBuffer};

const fn cmsg_align(len: usize) -> usize {
    (len + mem::align_of::<usize>() - 1) & !(mem::align_of::<usize>() - 1)
}

const fn cmsg_len(length: usize) -> usize {
    cmsg_align(mem::size_of::<WinSock::CMSGHDR>()) + length
}

const fn cmsg_space(length: usize) -> usize {
    cmsg_align(mem::size_of::<WinSock::CMSGHDR>()) + cmsg_align(length)
}

const CMSG_HDR_SIZE: usize = cmsg_len(0);

impl RecvAncillaryBuffer {
    pub fn parse_msg<T: RecvAncillaryData>(
        &self,
        msghdr: WinSock::WSAMSG,
        data: &mut T,
    ) -> io::Result<()> {
        self.parse(msghdr.Control.len as _, data)
    }

    #[allow(clippy::single_match)]
    pub fn parse_buf<T: RecvAncillaryData>(control_buf: &[u8], data: &mut T) -> io::Result<()> {
        let total_size = control_buf.len();
        let mut offset = 0usize;

        while offset + CMSG_HDR_SIZE <= total_size {
            let buf = &control_buf[offset..];
            let hdr = unsafe { buf.as_ptr().cast::<WinSock::CMSGHDR>().as_ref().unwrap() };
            if hdr.cmsg_len <= CMSG_HDR_SIZE {
                // empty record
                break;
            }
            if offset + hdr.cmsg_len > total_size {
                // too much payload data
                break;
            }
            offset += cmsg_space(hdr.cmsg_len - CMSG_HDR_SIZE);

            let payload = &buf[CMSG_HDR_SIZE..hdr.cmsg_len];

            match hdr.cmsg_level {
                WinSock::SOL_SOCKET => {}
                WinSock::IPPROTO_IP => match hdr.cmsg_type {
                    WinSock::IP_PKTINFO => {
                        if payload.len() < size_of::<WinSock::IN_PKTINFO>() {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "no enough msg data for struct IN_PKTINFO",
                            ));
                        }
                        let pktinfo = unsafe {
                            payload
                                .as_ptr()
                                .cast::<WinSock::IN_PKTINFO>()
                                .as_ref()
                                .unwrap()
                        };

                        data.set_recv_interface(pktinfo.ipi_ifindex);
                        let ip4 =
                            Ipv4Addr::from(u32::from_be(unsafe { pktinfo.ipi_addr.S_un.S_addr }));
                        data.set_recv_dst_addr(IpAddr::V4(ip4));
                    }
                    _ => {}
                },
                WinSock::IPPROTO_IPV6 => match hdr.cmsg_type {
                    WinSock::IPV6_PKTINFO => {
                        if payload.len() < size_of::<WinSock::IN6_PKTINFO>() {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "no enough msg data for struct IN6_PKTINFO",
                            ));
                        }
                        let pktinfo = unsafe {
                            payload
                                .as_ptr()
                                .cast::<WinSock::IN6_PKTINFO>()
                                .as_ref()
                                .unwrap()
                        };

                        data.set_recv_interface(pktinfo.ipi6_ifindex);
                        let ip6 = Ipv6Addr::from(unsafe { pktinfo.ipi6_addr.u.Byte });
                        data.set_recv_dst_addr(IpAddr::V6(ip6));
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
    pub(super) fn encode_v4(&mut self, ip: Ipv4Addr) -> Option<()> {
        let mut info = unsafe { mem::zeroed::<WinSock::IN_PKTINFO>() };
        unsafe { info.ipi_addr.S_un.S_addr = u32::from(ip).to_be() };
        self.write(
            WinSock::IPPROTO_IP,
            WinSock::IP_PKTINFO,
            std::ptr::from_ref(&info).cast(),
            size_of::<WinSock::IN_PKTINFO>(),
        )
    }

    pub(super) fn encode_v6(&mut self, ip: Ipv6Addr) -> Option<()> {
        let mut info = unsafe { mem::zeroed::<WinSock::IN6_PKTINFO>() };
        unsafe { info.ipi6_addr.u.Byte = ip.octets() };
        self.write(
            WinSock::IPPROTO_IPV6,
            WinSock::IPV6_PKTINFO,
            std::ptr::from_ref(&info).cast(),
            size_of::<WinSock::IN6_PKTINFO>(),
        )
    }

    fn write(
        &mut self,
        level: i32,
        cmsg_type: i32,
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
            let hdr = self.buf.as_mut_ptr().add(offset).cast::<WinSock::CMSGHDR>();
            (*hdr).cmsg_len = msg_len;
            (*hdr).cmsg_level = level;
            (*hdr).cmsg_type = cmsg_type;
            let data_off = cmsg_len(0);
            std::ptr::copy_nonoverlapping(data, hdr.cast::<u8>().add(data_off), data_len);
        }
        self.len = end;
        Some(())
    }
}
