/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

//! UDP SO_REUSEPORT BPF-based socket selector.
//!
//! This module provides [`UdpSocketSelector`] for loading and managing a BPF
//! program that steers incoming UDP datagrams to specific sockets using three
//! pinned BPF maps.

use std::fs;
use std::io;
use std::mem::MaybeUninit;
use std::net::{IpAddr, SocketAddr};
use std::os::fd::AsFd;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};

use anyhow::anyhow;
use libbpf_rs::skel::{OpenSkel, SkelBuilder};
use libbpf_rs::{MapCore, MapFlags, MapHandle};
use log::warn;
use zerocopy::{Immutable, IntoBytes};

#[allow(
    clippy::all,
    dead_code,
    non_snake_case,
    non_camel_case_types,
    missing_docs
)]
mod skel {
    include!(concat!(env!("OUT_DIR"), "/udp.skel.rs"));
}

use skel::UdpSkelBuilder;

#[derive(IntoBytes, Immutable)]
#[repr(C)]
struct SocketId {
    pid: u32,
    generation: u32,
    worker: u32,
}

#[derive(IntoBytes, Immutable)]
#[repr(C)]
struct ProcMapKey {
    pid: u32,
    generation: u32,
}

#[derive(IntoBytes, Immutable)]
#[repr(C)]
struct ProcMapValue {
    count: u32,
    invalid: u32,
}

pub struct UdpSocketSelector {
    pin_dir: PathBuf,
    conn_track_max_entries: u32,
    pid: u32,
    generation: u32,
    sockets: Vec<RawFd>,
    proc_map_handle: Option<MapHandle>,
    socket_map_handle: Option<MapHandle>,
}

impl UdpSocketSelector {
    pub fn pin_dir(&self) -> &Path {
        &self.pin_dir
    }

    pub fn new(
        pid: u32,
        generation: u32,
        addr: SocketAddr,
        conn_track_max_entries: u32,
    ) -> anyhow::Result<Self> {
        let ip = match addr.ip() {
            IpAddr::V4(ip) => ip.to_ipv6_compatible(), // IPv4 "." is not allowed in path
            IpAddr::V6(ip) => ip,
        };
        let dir = format!("/sys/fs/bpf/vey-reuseport/udp/{ip}_{}", addr.port());
        let pin_dir = PathBuf::from(dir);

        fs::create_dir_all(&pin_dir)
            .map_err(|e| anyhow!("failed to create pin directory {}: {e}", pin_dir.display()))?;

        Ok(UdpSocketSelector {
            pin_dir,
            conn_track_max_entries,
            pid,
            generation,
            sockets: Vec::new(),
            proc_map_handle: None,
            socket_map_handle: None,
        })
    }

    pub fn add_socket(&mut self, socket: RawFd) {
        self.sockets.push(socket);
    }

    pub fn load_and_attach(&mut self) -> anyhow::Result<()> {
        let mut open_object = MaybeUninit::uninit();
        let mut open_skel = UdpSkelBuilder::default()
            .open(&mut open_object)
            .map_err(|e| anyhow!("failed to open the udp socket selector ebpf object: {e}"))?;

        if let Some(d) = &mut open_skel.maps.rodata_data {
            d.load_pid = self.pid;
            d.load_generation = self.generation;
        }

        let mut conn_track_pin = true;
        let conn_track_path = self.pin_dir.join(open_skel.maps.conn_track.name());
        if let Ok(handle) = MapHandle::from_pinned_path(&conn_track_path) {
            if handle.max_entries() != self.conn_track_max_entries {
                warn!(
                    "udp conn_track map {} already pinned with max entries {}, delete it first if you want to set max entries to {}",
                    conn_track_path.display(),
                    handle.max_entries(),
                    self.conn_track_max_entries
                );
            }
            conn_track_pin = false;
            open_skel
                .maps
                .conn_track
                .reuse_fd(handle.as_fd())
                .map_err(|e| {
                    anyhow!(
                        "failed to reuse already pinned {}: {e}",
                        conn_track_path.display()
                    )
                })?;
        } else {
            open_skel
                .maps
                .conn_track
                .set_max_entries(self.conn_track_max_entries)
                .map_err(|e| anyhow!("failed to set max entries for conn_track map: {e}"))?;
        }

        let mut proc_map_pin = true;
        let proc_map_path = self.pin_dir.join(open_skel.maps.proc_map.name());
        if let Ok(handle) = MapHandle::from_pinned_path(&proc_map_path) {
            proc_map_pin = false;
            open_skel
                .maps
                .proc_map
                .reuse_fd(handle.as_fd())
                .map_err(|e| {
                    anyhow!(
                        "failed to reuse already pinned {}: {e}",
                        proc_map_path.display()
                    )
                })?;
        }

        let mut socket_map_pin = true;
        let socket_map_path = self.pin_dir.join(open_skel.maps.socket_map.name());
        if let Ok(handle) = MapHandle::from_pinned_path(&socket_map_path) {
            socket_map_pin = false;
            open_skel
                .maps
                .socket_map
                .reuse_fd(handle.as_fd())
                .map_err(|e| {
                    anyhow!(
                        "failed to reuse already pinned {}: {e}",
                        socket_map_path.display()
                    )
                })?;
        }

        // Load the BPF program into the kernel
        let mut skel = open_skel
            .load()
            .map_err(|e| anyhow!("failed to load to kernel: {e}"))?;

        if conn_track_pin {
            skel.maps
                .conn_track
                .pin(&conn_track_path)
                .map_err(|e| anyhow!("failed to pin conn_track map: {e}"))?;
        }
        if proc_map_pin {
            skel.maps
                .proc_map
                .pin(&proc_map_path)
                .map_err(|e| anyhow!("failed to pin proc_map map: {e}"))?;
        }
        if socket_map_pin {
            skel.maps
                .socket_map
                .pin(&socket_map_path)
                .map_err(|e| anyhow!("failed to pin socket map map: {e}"))?;
        }

        let socket_map_handle = MapHandle::from_pinned_path(&socket_map_path).map_err(|e| {
            anyhow!(
                "failed to open socket map {}: {e}",
                socket_map_path.display()
            )
        })?;
        self.socket_map_handle = Some(socket_map_handle);
        for (i, socket) in self.sockets.iter().enumerate() {
            let key = SocketId {
                pid: self.pid,
                generation: self.generation,
                worker: i as u32,
            };
            let value = *socket as u64;
            skel.maps
                .socket_map
                .update(key.as_bytes(), value.as_bytes(), MapFlags::NO_EXIST)
                .map_err(|e| anyhow!("failed to add #{i} socket {socket} to socket map: {e}"))?;
        }

        let proc_map_handle = MapHandle::from_pinned_path(&proc_map_path)
            .map_err(|e| anyhow!("failed to open proc map {}: {e}", proc_map_path.display()))?;
        self.proc_map_handle = Some(proc_map_handle);
        let key = ProcMapKey {
            pid: self.pid,
            generation: self.generation,
        };
        let value = ProcMapValue {
            count: self.sockets.len() as u32,
            invalid: 0,
        };
        skel.maps
            .proc_map
            .update(key.as_bytes(), value.as_bytes(), MapFlags::NO_EXIST)
            .map_err(|e| anyhow!("failed to add current proc to proc map: {e}"))?;

        if let Some(fd) = self.sockets.first() {
            let prog_fd = skel.progs.udp_select_reuseport.as_fd().as_raw_fd();
            attach_reuseport_ebpf(*fd, prog_fd)?;
        }

        Ok(())
    }

    pub fn unregister_proc(&mut self) {
        let Some(handle) = &self.proc_map_handle else {
            return;
        };
        let key = ProcMapKey {
            pid: self.pid,
            generation: self.generation,
        };
        let _ = handle.delete(key.as_bytes());
        self.proc_map_handle = None;
    }
}

impl Drop for UdpSocketSelector {
    fn drop(&mut self) {
        self.unregister_proc();
        if let Some(handle) = &self.socket_map_handle {
            for i in 0..self.sockets.len() {
                let key = SocketId {
                    pid: self.pid,
                    generation: self.generation,
                    worker: i as u32,
                };
                let _ = handle.delete(key.as_bytes());
            }
            self.socket_map_handle = None;
        }
    }
}

/// Attach a BPF program to a reuseport socket via `setsockopt`.
fn attach_reuseport_ebpf(socket_fd: RawFd, prog_fd: RawFd) -> io::Result<()> {
    let ret = unsafe {
        libc::setsockopt(
            socket_fd,
            libc::SOL_SOCKET,
            libc::SO_ATTACH_REUSEPORT_EBPF,
            &prog_fd as *const RawFd as *const libc::c_void,
            size_of::<RawFd>() as libc::socklen_t,
        )
    };
    if ret != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
