/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::future::poll_fn;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::task::{Context, Poll, ready};

use vey_io_ext::{AsyncUdpRecv, UdpRelayClientError, UdpRelayClientRecv};
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "macos",
    target_os = "solaris",
))]
use vey_io_ext::{UdpRelayPacket, UdpRelayPacketMeta};
use vey_socks::v5::UdpInput;
use vey_types::acl::{AclAction, AclNetworkRule};
use vey_types::net::UpstreamAddr;

use super::CommonTaskContext;
use crate::auth::UserContext;

pub(super) struct Socks5UdpAssociateClientRecv<T> {
    inner: T,
    client_addr: SocketAddr,
    ctx: Arc<CommonTaskContext>,
    user_ctx: Option<UserContext>,
}

impl<T> Socks5UdpAssociateClientRecv<T>
where
    T: AsyncUdpRecv,
{
    pub(super) fn new(
        inner: T,
        client: Option<SocketAddr>,
        ctx: &Arc<CommonTaskContext>,
        user_ctx: Option<&UserContext>,
    ) -> Self {
        let client_addr =
            client.unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0));
        Socks5UdpAssociateClientRecv {
            inner,
            client_addr,
            ctx: Arc::clone(ctx),
            user_ctx: user_ctx.cloned(),
        }
    }

    pub(super) fn inner(&self) -> &T {
        &self.inner
    }

    pub(super) fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    fn handle_user_upstream_acl_action(
        &self,
        action: AclAction,
    ) -> Result<(), UdpRelayClientError> {
        let forbid = match action {
            AclAction::Permit => false,
            AclAction::PermitAndLog => {
                // TODO log permit
                false
            }
            AclAction::Forbid => true,
            AclAction::ForbidAndLog => {
                // TODO log forbid
                true
            }
        };
        if forbid {
            Err(UdpRelayClientError::ForbiddenTargetAddress)
        } else {
            Ok(())
        }
    }

    fn handle_server_upstream_acl_action(
        &self,
        action: AclAction,
    ) -> Result<(), UdpRelayClientError> {
        let forbid = match action {
            AclAction::Permit => false,
            AclAction::PermitAndLog => {
                // TODO log permit
                false
            }
            AclAction::Forbid => true,
            AclAction::ForbidAndLog => {
                // TODO log forbid
                true
            }
        };
        if forbid {
            self.ctx.server_stats.forbidden.add_dest_denied();
            if let Some(user_ctx) = &self.user_ctx {
                // also add to user level forbidden stats
                user_ctx.add_dest_denied();
            }

            Err(UdpRelayClientError::ForbiddenTargetAddress)
        } else {
            Ok(())
        }
    }

    fn check_upstream(&self, upstream: &UpstreamAddr) -> Result<(), UdpRelayClientError> {
        if let Some(user_ctx) = &self.user_ctx {
            let action = user_ctx.check_upstream(upstream);
            self.handle_user_upstream_acl_action(action)?;
        }

        let action = self.ctx.check_upstream(upstream);
        self.handle_server_upstream_acl_action(action)?;

        Ok(())
    }

    fn parse_and_check_header(
        &self,
        buf: &[u8],
    ) -> Result<(usize, UpstreamAddr), UdpRelayClientError> {
        let (off, upstream) = UdpInput::parse_header(buf)
            .map_err(|e| UdpRelayClientError::InvalidPacket(e.to_string()))?;
        self.check_upstream(&upstream)?;
        Ok((off, upstream))
    }

    fn poll_recv(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, usize, UpstreamAddr), UdpRelayClientError>> {
        let nr = ready!(self.inner.poll_recv(cx, buf)).map_err(UdpRelayClientError::RecvFailed)?;

        let (off, upstream) = self.parse_and_check_header(&buf[..nr])?;
        Poll::Ready(Ok((off, nr, upstream)))
    }

    fn poll_recv_first(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
        ingress_net_filter: &Option<Arc<AclNetworkRule>>,
        initial_peer: &mut UpstreamAddr,
    ) -> Poll<Result<(usize, usize), UdpRelayClientError>> {
        let expected_ip = self.client_addr.ip();
        let expected_port = self.client_addr.port();
        let set_client = expected_ip.is_unspecified() || expected_port == 0;

        let (nr, client_addr) =
            ready!(self.inner.poll_recv_from(cx, buf)).map_err(UdpRelayClientError::RecvFailed)?;

        if set_client {
            if !expected_ip.is_unspecified() && expected_ip != client_addr.ip() {
                return Poll::Ready(Err(UdpRelayClientError::MismatchedClientAddress));
            }
            if expected_port != 0 && expected_port != client_addr.port() {
                // TODO log
            }
        } else if self.client_addr.ne(&client_addr) {
            return Poll::Ready(Err(UdpRelayClientError::MismatchedClientAddress));
        }

        if let Some(ingress_net_filter) = ingress_net_filter {
            let (_, action) = ingress_net_filter.check(client_addr.ip());
            match action {
                AclAction::Permit => {}
                AclAction::PermitAndLog => {
                    // TODO log
                }
                AclAction::Forbid => {
                    return Poll::Ready(Err(UdpRelayClientError::ForbiddenClientAddress));
                }
                AclAction::ForbidAndLog => {
                    // TODO log
                    return Poll::Ready(Err(UdpRelayClientError::ForbiddenClientAddress));
                }
            }
        }

        self.client_addr = client_addr;

        let (off, upstream) = UdpInput::parse_header(&buf[..nr])
            .map_err(|e| UdpRelayClientError::InvalidPacket(e.to_string()))?;
        *initial_peer = upstream;
        self.check_upstream(initial_peer)?;
        Poll::Ready(Ok((off, nr)))
    }

    pub async fn recv_first_packet(
        &mut self,
        buf: &mut [u8],
        ingress_net_filter: &Option<Arc<AclNetworkRule>>,
        initial_peer: &mut UpstreamAddr,
    ) -> Result<(usize, usize, SocketAddr), UdpRelayClientError> {
        loop {
            // only receive the first valid packet
            match poll_fn(|cx| self.poll_recv_first(cx, buf, ingress_net_filter, initial_peer))
                .await
            {
                Ok((off, nr)) => return Ok((off, nr, self.client_addr)),
                Err(UdpRelayClientError::MismatchedClientAddress) => {}
                Err(e) => return Err(e),
            }
        }
    }
}

impl<T> UdpRelayClientRecv for Socks5UdpAssociateClientRecv<T>
where
    T: AsyncUdpRecv + Send,
{
    /// reserve some space for offloading header
    fn max_hdr_len(&self) -> usize {
        256 + 4 + 2
    }

    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, usize, UpstreamAddr), UdpRelayClientError>> {
        self.poll_recv(cx, buf)
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    ))]
    fn poll_recv_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &mut [UdpRelayPacket],
    ) -> Poll<Result<usize, UdpRelayClientError>> {
        use vey_io_sys::udp::RecvMsgHdr;

        let mut hdr_v: Vec<RecvMsgHdr<1>> = packets
            .iter_mut()
            .map(|p| RecvMsgHdr::new([std::io::IoSliceMut::new(p.buf_mut())]))
            .collect();

        let count = ready!(self.inner.poll_batch_recvmsg(cx, &mut hdr_v))
            .map_err(UdpRelayClientError::RecvFailed)?;

        let mut r = Vec::with_capacity(count);
        for h in hdr_v.into_iter().take(count) {
            let iov = &h.iov[0];
            let (off, ups) = self.parse_and_check_header(&iov[0..h.n_recv])?;
            r.push(UdpRelayPacketMeta::new(iov, off, h.n_recv, ups))
        }
        for (m, p) in r.into_iter().zip(packets.iter_mut()) {
            m.set_packet(p);
        }

        Poll::Ready(Ok(count))
    }
}

#[cfg(all(
    test,
    any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    )
))]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::io;
    use std::net::SocketAddr;
    use std::task::Waker;
    use std::time::Duration;

    use vey_daemon::server::ClientConnectionInfo;
    use vey_io_ext::IdleWheel;
    use vey_io_sys::udp::RecvMsgHdr;
    use vey_socks::v5::UdpOutput;
    use vey_types::acl::{AclAction, AclExactHostRule};
    use vey_types::acl_set::AclDstHostRuleSetBuilder;
    use vey_types::metrics::NodeName;

    use super::super::SocksProxyServerStats;
    use crate::config::server::socks_proxy::SocksProxyServerConfig;
    use crate::serve::ServerQuitPolicy;

    const ALLOWED_IP: Ipv4Addr = Ipv4Addr::new(1, 1, 1, 1);
    const FORBIDDEN_IP: Ipv4Addr = Ipv4Addr::new(8, 8, 8, 8);

    struct MockBatchRecv {
        packets: VecDeque<Vec<u8>>,
    }

    impl MockBatchRecv {
        fn new(packets: impl IntoIterator<Item = Vec<u8>>) -> Self {
            MockBatchRecv {
                packets: packets.into_iter().collect(),
            }
        }
    }

    impl AsyncUdpRecv for MockBatchRecv {
        fn poll_recv_from(
            &mut self,
            _cx: &mut Context<'_>,
            _buf: &mut [u8],
        ) -> Poll<io::Result<(usize, SocketAddr)>> {
            unimplemented!("batch ACL tests only use poll_batch_recvmsg")
        }

        fn poll_recv(&mut self, _cx: &mut Context<'_>, _buf: &mut [u8]) -> Poll<io::Result<usize>> {
            unimplemented!("batch ACL tests only use poll_batch_recvmsg")
        }

        fn poll_recvmsg<const C: usize>(
            &mut self,
            _cx: &mut Context<'_>,
            _hdr: &mut RecvMsgHdr<'_, C>,
        ) -> Poll<io::Result<()>> {
            unimplemented!("batch ACL tests only use poll_batch_recvmsg")
        }

        fn poll_batch_recvmsg<const C: usize>(
            &mut self,
            _cx: &mut Context<'_>,
            hdr_v: &mut [RecvMsgHdr<'_, C>],
        ) -> Poll<io::Result<usize>> {
            let mut count = 0;
            for hdr in hdr_v.iter_mut() {
                let Some(data) = self.packets.pop_front() else {
                    break;
                };
                hdr.iov[0][..data.len()].copy_from_slice(&data);
                hdr.n_recv = data.len();
                count += 1;
            }
            if count == 0 {
                Poll::Pending
            } else {
                Poll::Ready(Ok(count))
            }
        }
    }

    fn socks_udp_packet(ip: Ipv4Addr, port: u16, payload: &[u8]) -> Vec<u8> {
        let ups = UpstreamAddr::from_ip_and_port(IpAddr::V4(ip), port);
        let hdr_len = UdpOutput::calc_header_len(&ups);
        let mut buf = vec![0u8; hdr_len + payload.len()];
        UdpOutput::generate_header(&mut buf[..hdr_len], &ups);
        buf[hdr_len..].copy_from_slice(payload);
        buf
    }

    fn recv_with_dst_acl(
        packets: impl IntoIterator<Item = Vec<u8>>,
    ) -> Socks5UdpAssociateClientRecv<MockBatchRecv> {
        let mut exact = AclExactHostRule::new(AclAction::Permit);
        exact.add_ip(IpAddr::V4(FORBIDDEN_IP), AclAction::Forbid);
        let dst_host_filter = AclDstHostRuleSetBuilder {
            exact: Some(exact),
            ..Default::default()
        }
        .build();

        let ctx = Arc::new(CommonTaskContext {
            server_config: Arc::new(SocksProxyServerConfig::new_for_test()),
            server_stats: Arc::new(SocksProxyServerStats::new(&NodeName::default())),
            server_quit_policy: Arc::new(ServerQuitPolicy::default()),
            idle_wheel: IdleWheel::spawn(Duration::from_secs(60)),
            escaper: crate::escape::get_or_insert_default(&NodeName::default()),
            ingress_net_filter: None,
            dst_host_filter: Some(Arc::new(dst_host_filter)),
            cc_info: ClientConnectionInfo::new(
                "127.0.0.1:12345".parse().unwrap(),
                "127.0.0.1:1080".parse().unwrap(),
            ),
            task_logger: None,
        });

        Socks5UdpAssociateClientRecv::new(MockBatchRecv::new(packets), None, &ctx, None)
    }

    #[test]
    fn batch_recv_rejects_forbidden_target_after_allowed_packet() {
        let allowed = socks_udp_packet(ALLOWED_IP, 53, b"ok");
        let forbidden = socks_udp_packet(FORBIDDEN_IP, 53, b"no");
        let mut recv = recv_with_dst_acl([allowed, forbidden]);
        let mut packets = vec![UdpRelayPacket::new(0, 512); 2];
        let mut cx = Context::from_waker(Waker::noop());

        let err = match recv.poll_recv_packets(&mut cx, &mut packets) {
            Poll::Ready(Err(e)) => e,
            other => panic!("expected forbidden target, got {other:?}"),
        };
        assert!(matches!(err, UdpRelayClientError::ForbiddenTargetAddress));
        assert!(packets[0].upstream().is_empty());
        assert!(packets[1].upstream().is_empty());
    }

    #[test]
    fn batch_recv_accepts_allowed_targets() {
        let first = socks_udp_packet(ALLOWED_IP, 53, b"one");
        let second = socks_udp_packet(ALLOWED_IP, 853, b"two");
        let mut recv = recv_with_dst_acl([first, second]);
        let mut packets = vec![UdpRelayPacket::new(0, 512); 2];
        let mut cx = Context::from_waker(Waker::noop());

        let count = match recv.poll_recv_packets(&mut cx, &mut packets) {
            Poll::Ready(Ok(n)) => n,
            other => panic!("expected two allowed packets, got {other:?}"),
        };
        assert_eq!(count, 2);
        assert_eq!(
            packets[0].upstream(),
            &UpstreamAddr::from_ip_and_port(IpAddr::V4(ALLOWED_IP), 53)
        );
        assert_eq!(packets[0].payload(), b"one");
        assert_eq!(
            packets[1].upstream(),
            &UpstreamAddr::from_ip_and_port(IpAddr::V4(ALLOWED_IP), 853)
        );
        assert_eq!(packets[1].payload(), b"two");
    }
}
