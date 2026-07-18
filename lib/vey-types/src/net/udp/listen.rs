/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

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
struct NonReloadablePart {
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
    instance: usize,
    scale: usize,
    #[cfg(target_os = "linux")]
    use_ebpf: Option<bool>,
    #[cfg(target_os = "linux")]
    fail_on_ebpf_error: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReloadablePart {
    buf_conf: SocketBufferConfig,
    misc_opts: UdpMiscSockOpts,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UdpListenConfig {
    non_reloadable: NonReloadablePart,
    reloadable: ReloadablePart,
}

impl Default for UdpListenConfig {
    fn default() -> Self {
        UdpListenConfig::new(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0))
    }
}

impl UdpListenConfig {
    pub fn new(address: SocketAddr) -> Self {
        UdpListenConfig {
            non_reloadable: NonReloadablePart {
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
                instance: 1,
                scale: 0,
                #[cfg(target_os = "linux")]
                use_ebpf: None,
                #[cfg(target_os = "linux")]
                fail_on_ebpf_error: false,
            },
            reloadable: ReloadablePart {
                buf_conf: SocketBufferConfig::default(),
                misc_opts: UdpMiscSockOpts::default(),
            },
        }
    }

    pub fn check(&mut self) -> anyhow::Result<()> {
        if self.non_reloadable.address.port() == 0 {
            return Err(anyhow!("no listen port is set"));
        }
        #[cfg(not(target_os = "openbsd"))]
        match self.non_reloadable.address.ip() {
            IpAddr::V4(_) => self.non_reloadable.ipv6only = None,
            IpAddr::V6(v6) => {
                if !v6.is_unspecified() {
                    self.non_reloadable.ipv6only = None;
                }
            }
        }

        Ok(())
    }

    pub fn need_respawn(&self, other: &Self) -> bool {
        self.non_reloadable != other.non_reloadable
    }

    pub fn need_reloadable_change(&self, other: &Self) -> bool {
        self.reloadable != other.reloadable
    }

    #[inline]
    pub fn address(&self) -> SocketAddr {
        self.non_reloadable.address
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
        self.non_reloadable.interface.as_ref()
    }

    #[inline]
    pub fn socket_buffer(&self) -> SocketBufferConfig {
        self.reloadable.buf_conf
    }

    #[inline]
    pub fn socket_misc_opts(&self) -> UdpMiscSockOpts {
        self.reloadable.misc_opts
    }

    #[cfg(not(target_os = "openbsd"))]
    #[inline]
    pub fn is_ipv6only(&self) -> Option<bool> {
        self.non_reloadable.ipv6only
    }

    #[cfg(target_os = "linux")]
    #[inline]
    pub fn transparent(&self) -> bool {
        self.non_reloadable.transparent
    }

    #[inline]
    pub fn instance(&self) -> usize {
        self.non_reloadable.instance.max(self.non_reloadable.scale)
    }

    #[inline]
    pub fn set_socket_address(&mut self, addr: SocketAddr) {
        self.non_reloadable.address = addr;
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
        self.non_reloadable.interface = Some(interface);
    }

    #[inline]
    pub fn set_socket_buffer(&mut self, buf_conf: SocketBufferConfig) {
        self.reloadable.buf_conf = buf_conf;
    }

    #[inline]
    pub fn set_socket_misc_opts(&mut self, misc_opts: UdpMiscSockOpts) {
        self.reloadable.misc_opts = misc_opts;
    }

    #[inline]
    pub fn set_port(&mut self, port: u16) {
        self.non_reloadable.address.set_port(port);
    }

    #[cfg(not(target_os = "openbsd"))]
    #[inline]
    pub fn set_ipv6_only(&mut self, ipv6only: bool) {
        self.non_reloadable.ipv6only = Some(ipv6only);
    }

    #[cfg(target_os = "linux")]
    #[inline]
    pub fn set_transparent(&mut self) {
        self.non_reloadable.transparent = true;
    }

    pub fn set_instance(&mut self, instance: usize) {
        if instance == 0 {
            self.non_reloadable.instance = 1;
        } else {
            self.non_reloadable.instance = instance;
        }
    }

    pub fn set_scale(&mut self, scale: f64) -> anyhow::Result<()> {
        if let Ok(p) = std::thread::available_parallelism() {
            let v = (p.get() as f64) * scale;
            self.non_reloadable.scale = v
                .round()
                .to_usize()
                .ok_or(anyhow!("out of range result: {v}"))?;
        }
        Ok(())
    }

    pub fn set_fraction_scale(&mut self, numerator: usize, denominator: usize) {
        if let Ok(p) = std::thread::available_parallelism() {
            let v = p.get() * numerator / denominator;
            self.non_reloadable.scale = v;
        }
    }

    #[cfg(target_os = "linux")]
    pub fn use_ebpf(&self, uid: u32) -> bool {
        self.non_reloadable.use_ebpf.unwrap_or(uid == 0)
    }

    #[cfg(target_os = "linux")]
    pub fn set_use_ebpf(&mut self, use_ebpf: bool) {
        self.non_reloadable.use_ebpf = Some(use_ebpf);
    }

    #[cfg(target_os = "linux")]
    #[inline]
    pub fn fail_on_ebpf_error(&self) -> bool {
        self.non_reloadable.fail_on_ebpf_error
    }

    #[cfg(target_os = "linux")]
    pub fn set_fail_on_ebpf_error(&mut self, fail: bool) {
        self.non_reloadable.fail_on_ebpf_error = fail;
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct UdpConnectionTrackConfig {
    max_sessions: NonZeroUsize,
    ebpf_conn_track_size: NonZeroU32,
    dispatch_queue_size: NonZeroUsize,
    send_queue_size: NonZeroUsize,
    batch_recv_size: NonZeroUsize,
    offline_wait_time: Duration,
    offline_quit_time: Duration,
}

impl Default for UdpConnectionTrackConfig {
    fn default() -> Self {
        UdpConnectionTrackConfig {
            max_sessions: unsafe { NonZeroUsize::new_unchecked(32768) },
            ebpf_conn_track_size: unsafe { NonZeroU32::new_unchecked(1048576) },
            dispatch_queue_size: unsafe { NonZeroUsize::new_unchecked(32) },
            send_queue_size: unsafe { NonZeroUsize::new_unchecked(512) },
            batch_recv_size: unsafe { NonZeroUsize::new_unchecked(16) },
            offline_wait_time: Duration::from_secs(60),
            offline_quit_time: Duration::from_hours(1),
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

    #[inline]
    pub fn offline_wait_time(&self) -> Duration {
        self.offline_wait_time
    }

    #[inline]
    pub fn set_offline_wait_time(&mut self, wait_time: Duration) {
        self.offline_wait_time = wait_time;
    }

    #[inline]
    pub fn offline_quit_time(&self) -> Duration {
        self.offline_quit_time
    }

    #[inline]
    pub fn set_offline_quit_time(&mut self, quit_time: Duration) {
        self.offline_quit_time = quit_time;
    }
}
