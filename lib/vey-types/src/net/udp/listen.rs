/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::num::{NonZeroU32, NonZeroUsize};

use anyhow::anyhow;
use num_traits::ToPrimitive;

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "illumos",
    target_os = "solaris"
))]
use crate::net::Interface;
use crate::net::{SocketBufferConfig, UdpMiscSockOpts};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UdpListenConfig {
    address: SocketAddr,
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "illumos",
        target_os = "solaris"
    ))]
    interface: Option<Interface>,
    #[cfg(not(target_os = "openbsd"))]
    ipv6only: Option<bool>,
    #[cfg(target_os = "linux")]
    transparent: bool,
    buf_conf: SocketBufferConfig,
    misc_opts: UdpMiscSockOpts,
    instance: usize,
    scale: usize,
    #[cfg(target_os = "linux")]
    use_ebpf: Option<bool>,
    #[cfg(target_os = "linux")]
    fail_on_ebpf_error: bool,
}

impl Default for UdpListenConfig {
    fn default() -> Self {
        UdpListenConfig::new(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0))
    }
}

impl UdpListenConfig {
    pub fn new(address: SocketAddr) -> Self {
        UdpListenConfig {
            address,
            #[cfg(any(
                target_os = "linux",
                target_os = "android",
                target_os = "macos",
                target_os = "illumos",
                target_os = "solaris"
            ))]
            interface: None,
            #[cfg(not(target_os = "openbsd"))]
            ipv6only: None,
            #[cfg(target_os = "linux")]
            transparent: false,
            buf_conf: SocketBufferConfig::default(),
            misc_opts: UdpMiscSockOpts::default(),
            instance: 1,
            scale: 0,
            #[cfg(target_os = "linux")]
            use_ebpf: None,
            #[cfg(target_os = "linux")]
            fail_on_ebpf_error: false,
        }
    }

    pub fn check(&mut self) -> anyhow::Result<()> {
        if self.address.port() == 0 {
            return Err(anyhow!("no listen port is set"));
        }
        #[cfg(not(target_os = "openbsd"))]
        match self.address.ip() {
            IpAddr::V4(_) => self.ipv6only = None,
            IpAddr::V6(v6) => {
                if !v6.is_unspecified() {
                    self.ipv6only = None;
                }
            }
        }

        Ok(())
    }

    pub fn need_respawn(&self, other: &Self) -> bool {
        if self.address != other.address {
            return true;
        }
        if self.instance() != other.instance() {
            return true;
        }
        #[cfg(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "illumos",
            target_os = "solaris"
        ))]
        if self.interface != other.interface {
            return true;
        }
        #[cfg(not(target_os = "openbsd"))]
        if self.ipv6only != other.ipv6only {
            return true;
        }
        #[cfg(target_os = "linux")]
        if self.transparent != other.transparent {
            return true;
        }
        #[cfg(target_os = "linux")]
        if self.use_ebpf != other.use_ebpf || self.fail_on_ebpf_error != other.fail_on_ebpf_error {
            return true;
        }

        false
    }

    #[inline]
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "illumos",
        target_os = "solaris"
    ))]
    #[inline]
    pub fn interface(&self) -> Option<&Interface> {
        self.interface.as_ref()
    }

    #[inline]
    pub fn socket_buffer(&self) -> SocketBufferConfig {
        self.buf_conf
    }

    #[inline]
    pub fn socket_misc_opts(&self) -> UdpMiscSockOpts {
        self.misc_opts
    }

    #[cfg(not(target_os = "openbsd"))]
    #[inline]
    pub fn is_ipv6only(&self) -> Option<bool> {
        self.ipv6only
    }

    #[cfg(target_os = "linux")]
    #[inline]
    pub fn transparent(&self) -> bool {
        self.transparent
    }

    #[inline]
    pub fn instance(&self) -> usize {
        self.instance.max(self.scale)
    }

    #[inline]
    pub fn set_socket_address(&mut self, addr: SocketAddr) {
        self.address = addr;
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "illumos",
        target_os = "solaris"
    ))]
    #[inline]
    pub fn set_interface(&mut self, interface: Interface) {
        self.interface = Some(interface);
    }

    #[inline]
    pub fn set_socket_buffer(&mut self, buf_conf: SocketBufferConfig) {
        self.buf_conf = buf_conf;
    }

    #[inline]
    pub fn set_socket_misc_opts(&mut self, misc_opts: UdpMiscSockOpts) {
        self.misc_opts = misc_opts;
    }

    #[inline]
    pub fn set_port(&mut self, port: u16) {
        self.address.set_port(port);
    }

    #[cfg(not(target_os = "openbsd"))]
    #[inline]
    pub fn set_ipv6_only(&mut self, ipv6only: bool) {
        self.ipv6only = Some(ipv6only);
    }

    #[cfg(target_os = "linux")]
    #[inline]
    pub fn set_transparent(&mut self) {
        self.transparent = true;
    }

    pub fn set_instance(&mut self, instance: usize) {
        if instance == 0 {
            self.instance = 1;
        } else {
            self.instance = instance;
        }
    }

    pub fn set_scale(&mut self, scale: f64) -> anyhow::Result<()> {
        if let Ok(p) = std::thread::available_parallelism() {
            let v = (p.get() as f64) * scale;
            self.scale = v
                .round()
                .to_usize()
                .ok_or(anyhow!("out of range result: {v}"))?;
        }
        Ok(())
    }

    pub fn set_fraction_scale(&mut self, numerator: usize, denominator: usize) {
        if let Ok(p) = std::thread::available_parallelism() {
            let v = p.get() * numerator / denominator;
            self.scale = v;
        }
    }

    #[cfg(target_os = "linux")]
    pub fn use_ebpf(&self, uid: u32) -> bool {
        self.use_ebpf.unwrap_or(uid == 0)
    }

    #[cfg(target_os = "linux")]
    pub fn set_use_ebpf(&mut self, use_ebpf: bool) {
        self.use_ebpf = Some(use_ebpf);
    }

    #[cfg(target_os = "linux")]
    #[inline]
    pub fn fail_on_ebpf_error(&self) -> bool {
        self.fail_on_ebpf_error
    }

    #[cfg(target_os = "linux")]
    pub fn set_fail_on_ebpf_error(&mut self, fail: bool) {
        self.fail_on_ebpf_error = fail;
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct UdpConnectionTrackConfig {
    max_sessions: NonZeroUsize,
    ebpf_conn_track_size: NonZeroU32,
    dispatch_queue_size: NonZeroUsize,
    send_queue_size: NonZeroUsize,
    batch_recv_size: NonZeroUsize,
}

impl Default for UdpConnectionTrackConfig {
    fn default() -> Self {
        UdpConnectionTrackConfig {
            max_sessions: unsafe { NonZeroUsize::new_unchecked(32768) },
            ebpf_conn_track_size: unsafe { NonZeroU32::new_unchecked(1048576) },
            dispatch_queue_size: unsafe { NonZeroUsize::new_unchecked(32) },
            send_queue_size: unsafe { NonZeroUsize::new_unchecked(512) },
            batch_recv_size: unsafe { NonZeroUsize::new_unchecked(16) },
        }
    }
}

impl UdpConnectionTrackConfig {
    #[inline]
    pub fn max_sessions(&self) -> NonZeroUsize {
        self.max_sessions
    }

    #[inline]
    pub fn set_max_sessions(&mut self, max_sessions: NonZeroUsize) {
        self.max_sessions = max_sessions;
    }

    #[inline]
    pub fn ebpf_conn_track_size(&self) -> NonZeroU32 {
        self.ebpf_conn_track_size
    }

    #[inline]
    pub fn set_ebpf_conn_track_size(&mut self, ebpf_conn_track_size: NonZeroU32) {
        self.ebpf_conn_track_size = ebpf_conn_track_size;
    }

    #[inline]
    pub fn dispatch_queue_size(&self) -> usize {
        self.dispatch_queue_size.get()
    }

    #[inline]
    pub fn set_dispatch_queue_size(&mut self, queue_size: NonZeroUsize) {
        self.dispatch_queue_size = queue_size;
    }

    #[inline]
    pub fn send_queue_size(&self) -> usize {
        self.send_queue_size.get()
    }

    #[inline]
    pub fn set_send_queue_size(&mut self, queue_size: NonZeroUsize) {
        self.send_queue_size = queue_size;
    }

    #[inline]
    pub fn batch_recv_size(&self) -> usize {
        self.batch_recv_size.get()
    }

    #[inline]
    pub fn set_batch_recv_size(&mut self, batch_recv_size: NonZeroUsize) {
        self.batch_recv_size = batch_recv_size;
    }
}
