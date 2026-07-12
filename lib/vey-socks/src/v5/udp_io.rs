/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use vey_types::net::{Host, UpstreamAddr};

use bytes::{Buf, BufMut};

use super::SocksUdpPacketError;

pub(crate) const UDP_HEADER_LEN_IPV4: usize = 10;
pub(crate) const UDP_HEADER_LEN_IPV6: usize = 22;

pub struct UdpInput {}

impl UdpInput {
    pub fn parse_header(buf: &[u8]) -> Result<(usize, UpstreamAddr), SocksUdpPacketError> {
        let len = buf.len();
        if len <= 8 {
            return Err(SocksUdpPacketError::TooSmallPacket);
        }

        if buf[0] != 0x00 || buf[1] != 0x00 {
            return Err(SocksUdpPacketError::ReservedNotZeroed);
        }

        if buf[2] != 0x00 {
            return Err(SocksUdpPacketError::FragmentNotSupported);
        }

        let (off, addr) = match buf[3] {
            0x01 => {
                if len < UDP_HEADER_LEN_IPV4 {
                    return Err(SocksUdpPacketError::TooSmallPacket);
                }

                let mut buf = &buf[4..];
                let ip4 = Ipv4Addr::from(buf.get_u32());
                let port = buf.get_u16();
                (
                    UDP_HEADER_LEN_IPV4,
                    UpstreamAddr::from_ip_and_port(IpAddr::V4(ip4), port),
                )
            }
            0x03 => {
                let domain_len = buf[4] as usize;
                let header_len = 4 + 1 + domain_len + 2;
                if len < header_len {
                    return Err(SocksUdpPacketError::TooSmallPacket);
                }

                let domain = std::str::from_utf8(&buf[5..5 + domain_len])
                    .map_err(|_| SocksUdpPacketError::InvalidDomainString)?;
                let port_off = 5 + domain_len;
                let port = ((buf[port_off] as u16) << 8) + buf[port_off + 1] as u16;
                let addr = UpstreamAddr::from_host_str_and_port(domain, port)
                    .map_err(|_| SocksUdpPacketError::InvalidDomainString)?;
                (header_len, addr)
            }
            0x04 => {
                if len < UDP_HEADER_LEN_IPV6 {
                    return Err(SocksUdpPacketError::TooSmallPacket);
                }

                let mut buf = &buf[4..];
                let ip6 = Ipv6Addr::from(buf.get_u128());
                let port = buf.get_u16();
                (
                    UDP_HEADER_LEN_IPV6,
                    UpstreamAddr::from_ip_and_port(IpAddr::V6(ip6), port),
                )
            }
            _ => return Err(SocksUdpPacketError::InvalidAddrType),
        };

        Ok((off, addr))
    }
}

pub struct UdpOutput {}

impl UdpOutput {
    pub fn calc_header_len(upstream: &UpstreamAddr) -> usize {
        match upstream.host() {
            Host::Ip(ip) => match ip {
                IpAddr::V6(ip6) => match ip6.to_ipv4_mapped() {
                    Some(_) => UDP_HEADER_LEN_IPV4,
                    None => UDP_HEADER_LEN_IPV6,
                },
                IpAddr::V4(_) => UDP_HEADER_LEN_IPV4,
            },
            Host::Domain(domain) => 5 + domain.len_u8() as usize + 2,
        }
    }

    /// the buf len should be equal to the result of calc_header_len()
    pub fn generate_header(mut buf: &mut [u8], upstream: &UpstreamAddr) {
        buf.put_u16(0x00);
        buf.put_u8(0x00);
        match upstream.host() {
            Host::Ip(ip) => Self::put_addr(buf, *ip, upstream.port()),
            Host::Domain(domain) => {
                buf.put_u8(0x03);
                let domain_len = domain.len_u8();
                buf.put_u8(domain_len);
                buf.put_slice(&domain.as_bytes()[0..domain_len as usize]);
                buf.put_u16(upstream.port());
            }
        }
    }

    pub fn generate_header2(mut buf: &mut [u8], addr: SocketAddr) {
        buf.put_u16(0x00);
        buf.put_u8(0x00);
        Self::put_addr(buf, addr.ip(), addr.port());
    }

    fn put_addr(mut buf: &mut [u8], ip: IpAddr, port: u16) {
        match ip {
            IpAddr::V4(ip4) => {
                buf.put_u8(0x01);
                buf.put_slice(&ip4.octets());
                buf.put_u16(port);
            }
            IpAddr::V6(ip6) => match ip6.to_ipv4_mapped() {
                Some(ip4) => {
                    buf.put_u8(0x01);
                    buf.put_slice(&ip4.octets());
                    buf.put_u16(port);
                }
                None => {
                    buf.put_u8(0x04);
                    buf.put_slice(&ip6.octets());
                    buf.put_u16(port);
                }
            },
        }
    }
}

#[derive(Clone)]
pub struct SocksUdpHeader {
    buf: Vec<u8>,
}

impl SocksUdpHeader {
    pub fn encode(&mut self, ups: &UpstreamAddr) -> &[u8] {
        let header_len = UdpOutput::calc_header_len(ups);
        if header_len > self.buf.len() {
            self.buf.resize(header_len, 0);
        }
        UdpOutput::generate_header(&mut self.buf, ups);
        &self.buf[0..header_len]
    }
}

impl Default for SocksUdpHeader {
    fn default() -> Self {
        SocksUdpHeader {
            buf: vec![0; 22], // large enough for ipv6
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn assert_round_trip(upstream: UpstreamAddr) {
        let header_len = UdpOutput::calc_header_len(&upstream);
        let mut buf = vec![0xff; header_len];

        UdpOutput::generate_header(&mut buf, &upstream);
        let (parsed_len, parsed_upstream) = UdpInput::parse_header(&buf).unwrap();

        assert_eq!(parsed_len, header_len);
        assert_eq!(parsed_upstream, upstream);
    }

    #[test]
    fn ipv4_header_round_trips() {
        assert_round_trip(UpstreamAddr::from_host_str_and_port("192.0.2.10", 8080).unwrap());
    }

    #[test]
    fn ipv6_header_round_trips() {
        assert_round_trip(UpstreamAddr::from_host_str_and_port("2001:db8::1", 443).unwrap());
    }

    #[test]
    fn ipv4_mapped_ipv6_uses_ipv4_header() {
        let upstream = UpstreamAddr::from_host_str_and_port("::ffff:192.0.2.10", 53).unwrap();
        let header_len = UdpOutput::calc_header_len(&upstream);
        let mut buf = vec![0; header_len];

        UdpOutput::generate_header(&mut buf, &upstream);

        assert_eq!(header_len, UDP_HEADER_LEN_IPV4);
        assert_eq!(buf[3], 0x01);
        assert_eq!(&buf[4..8], &[192, 0, 2, 10]);
        assert_eq!(&buf[8..10], &[0, 53]);
    }

    #[test]
    fn domain_header_round_trips() {
        let domain = "example.com".to_string();
        let upstream = UpstreamAddr::from_host_str_and_port(&domain, 8443).unwrap();
        let header_len = UdpOutput::calc_header_len(&upstream);
        let mut buf = vec![0; header_len];

        UdpOutput::generate_header(&mut buf, &upstream);
        let (parsed_len, parsed_upstream) = UdpInput::parse_header(&buf).unwrap();

        assert_eq!(parsed_len, header_len);
        assert_eq!(buf[3], 0x03);
        assert_eq!(buf[4], domain.len() as u8);
        assert_eq!(parsed_upstream.port(), 8443);
        assert_eq!(parsed_upstream.host().to_string(), domain);
    }

    #[test]
    fn generate_header2_encodes_socket_addr() {
        let addr: SocketAddr = "127.0.0.1:5353".parse().unwrap();
        let mut buf = vec![0; UDP_HEADER_LEN_IPV4];

        UdpOutput::generate_header2(&mut buf, addr);

        assert_eq!(buf, vec![0, 0, 0, 1, 127, 0, 0, 1, 20, 233]);
    }

    #[test]
    fn reusable_header_resizes_for_domain_then_shrinks_view_for_ipv4() {
        let domain = UpstreamAddr::from_host_str_and_port("example.com", 443).unwrap();
        let ipv4 = UpstreamAddr::from_host_str_and_port("198.51.100.7", 80).unwrap();
        let mut header = SocksUdpHeader::default();

        let domain_buf = header.encode(&domain);
        assert_eq!(domain_buf.len(), UdpOutput::calc_header_len(&domain));
        assert_eq!(domain_buf[3], 0x03);

        let ipv4_buf = header.encode(&ipv4);
        assert_eq!(ipv4_buf.len(), UDP_HEADER_LEN_IPV4);
        assert_eq!(ipv4_buf, &[0, 0, 0, 1, 198, 51, 100, 7, 0, 80]);
    }

    #[test]
    fn parse_header_rejects_malformed_packets() {
        assert!(matches!(
            UdpInput::parse_header(&[0; 8]),
            Err(SocksUdpPacketError::TooSmallPacket)
        ));
        assert!(matches!(
            UdpInput::parse_header(&[1, 0, 0, 1, 127, 0, 0, 1, 0, 80]),
            Err(SocksUdpPacketError::ReservedNotZeroed)
        ));
        assert!(matches!(
            UdpInput::parse_header(&[0, 0, 1, 1, 127, 0, 0, 1, 0, 80]),
            Err(SocksUdpPacketError::FragmentNotSupported)
        ));
        assert!(matches!(
            UdpInput::parse_header(&[0, 0, 0, 0xff, 127, 0, 0, 1, 0, 80]),
            Err(SocksUdpPacketError::InvalidAddrType)
        ));
        assert!(matches!(
            UdpInput::parse_header(&[0, 0, 0, 3, 4, b't', b'e', b's']),
            Err(SocksUdpPacketError::TooSmallPacket)
        ));
        assert!(matches!(
            UdpInput::parse_header(&[0, 0, 0, 3, 1, 0xff, 0, 0, 80]),
            Err(SocksUdpPacketError::InvalidDomainString)
        ));
    }
}
