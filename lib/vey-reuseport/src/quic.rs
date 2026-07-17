/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, SocketAddr};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::{fs, ptr};

use anyhow::anyhow;
use libbpf_rs::{AsRawLibbpf, MapCore, MapFlags, MapHandle, OpenObject};
use log::debug;
use zerocopy::IntoBytes;

use vey_socket::RawSocket;

use super::{ProcMapKey, ProcMapValue, ReadOnlyData, SocketId};

const NAME_OBJECT: &str = "quic_bpf";
const NAME_OBJECT_RODATA: &str = "quic_bpf.rodata";
const NAME_UDP_CONN_TRACK: &str = "udp_conn_track";
const NAME_QUIC_CONN_TRACK: &str = "quic_conn_track";
const NAME_SOCKET_MAP: &str = "socket_map";
const NAME_PROC_MAP: &str = "proc_map";
const NAME_PROGRAM: &str = "quic_select_reuseport";

#[unsafe(link_section = ".bpf.objs")]
static BPF_DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/quic.bpf.o"));

#[must_use]
pub struct QuicSocketSelectGuard {
    pid: i32,
    generation: u16,
    worker: u16,
    cookie: u64,
    socket_map_path: PathBuf,
    quic_conn_track_path: PathBuf,
}

impl QuicSocketSelectGuard {
    #[inline]
    pub fn cookie(&self) -> u64 {
        self.cookie
    }
}

impl Drop for QuicSocketSelectGuard {
    fn drop(&mut self) {
        if let Ok(handle) = MapHandle::from_pinned_path(&self.quic_conn_track_path)
            && handle.delete(self.cookie.as_bytes()).is_ok()
        {
            debug!(
                "deleted cookie {} from bpf map {}",
                self.cookie,
                self.socket_map_path.display()
            );
        }
        if let Ok(handle) = MapHandle::from_pinned_path(&self.socket_map_path) {
            let key = SocketId {
                pid: self.pid,
                generation: self.generation,
                worker: self.worker,
            };
            if handle.delete(key.as_bytes()).is_ok() {
                debug!(
                    "deleted pid:{}-generation:{}-worker:{} from bpf map {}",
                    self.pid,
                    self.generation,
                    self.worker,
                    self.socket_map_path.display()
                );
            }
        }
    }
}

pub struct QuicSocketSelector {
    pin_dir: PathBuf,
    pid: i32,
    generation: u16,
    sockets: Vec<(RawSocket, u64)>,
    quic_conn_track_path: PathBuf,
    quic_conn_track_handle: Option<MapHandle>,
    proc_map_handle: Option<MapHandle>,
    socket_map_path: PathBuf,
    socket_map_handle: Option<MapHandle>,
}

impl QuicSocketSelector {
    pub fn pin_dir(&self) -> &Path {
        &self.pin_dir
    }

    pub fn new(pid: i32, generation: u16, addr: SocketAddr) -> anyhow::Result<Self> {
        let ip = match addr.ip() {
            IpAddr::V4(ip) => ip.to_ipv6_compatible(), // IPv4 "." is not allowed in path
            IpAddr::V6(ip) => ip,
        };
        let dir = format!("/sys/fs/bpf/vey-reuseport/quic/{ip}_{}", addr.port());
        let pin_dir = PathBuf::from(dir);
        let quic_conn_track_path = pin_dir.join(NAME_QUIC_CONN_TRACK);
        let socket_map_path = pin_dir.join(NAME_SOCKET_MAP);

        fs::create_dir_all(&pin_dir)
            .map_err(|e| anyhow!("failed to create pin directory {}: {e}", pin_dir.display()))?;

        Ok(QuicSocketSelector {
            pin_dir,
            pid,
            generation,
            sockets: Vec::new(),
            quic_conn_track_path,
            quic_conn_track_handle: None,
            proc_map_handle: None,
            socket_map_path,
            socket_map_handle: None,
        })
    }

    pub fn add_socket(&mut self, socket: RawSocket) -> anyhow::Result<QuicSocketSelectGuard> {
        let cookie = socket
            .so_cookie()
            .map_err(|e| anyhow!("failed to get socket cookie: {e}"))?;
        self.sockets.push((socket.clone(), cookie));
        Ok(QuicSocketSelectGuard {
            pid: self.pid,
            generation: self.generation,
            worker: self.sockets.len() as u16 - 1,
            cookie,
            quic_conn_track_path: self.quic_conn_track_path.clone(),
            socket_map_path: self.socket_map_path.clone(),
        })
    }

    fn open_object(&mut self) -> anyhow::Result<OpenObject> {
        let mut builder = libbpf_rs::__internal_skel::ObjectSkeletonConfigBuilder::new(BPF_DATA);
        builder
            .name(NAME_OBJECT)
            .map(NAME_OBJECT_RODATA, true)
            .map(NAME_UDP_CONN_TRACK, false)
            .map(NAME_QUIC_CONN_TRACK, false)
            .map(NAME_SOCKET_MAP, false)
            .map(NAME_PROC_MAP, false)
            .prog(NAME_PROGRAM);
        let skel_config = builder
            .build()
            .map_err(|e| anyhow!("failed to build skel config: {e}"))?;

        let skel_ptr = skel_config.as_libbpf_object();
        let ret = unsafe { libbpf_sys::bpf_object__open_skeleton(skel_ptr.as_ptr(), ptr::null()) };
        if ret != 0 {
            return Err(anyhow!(
                "bpf_object__open_skeleton error: {}",
                libbpf_rs::Error::from_raw_os_error(-ret)
            ));
        }
        let obj_ptr = unsafe { *skel_ptr.as_ref().obj };
        let obj_ptr = ptr::NonNull::new(obj_ptr).unwrap();
        let open_object = unsafe { OpenObject::from_ptr(obj_ptr) };

        if let Some(ro_data) = unsafe {
            skel_config
                .map_mmap_ptr(0) // The first map is `rodata`
                .expect("BPF map `rodata` does not have mmap pointer")
                .cast::<ReadOnlyData>()
                .as_mut()
        } {
            ro_data.load_pid = self.pid;
            ro_data.load_generation = self.generation as u32;
        }

        Ok(open_object)
    }

    pub fn load_and_attach(&mut self) -> anyhow::Result<()> {
        let mut open_object = self.open_object()?;

        let mut udp_conn_track_pin = true;
        let udp_conn_track_path = self.pin_dir.join(NAME_UDP_CONN_TRACK);
        let proc_map_path = self.pin_dir.join(NAME_PROC_MAP);

        for mut map in open_object.maps_mut() {
            let Some(name) = map.name().to_str() else {
                continue;
            };
            match name {
                NAME_UDP_CONN_TRACK => {
                    if let Ok(handle) = MapHandle::from_pinned_path(&udp_conn_track_path) {
                        udp_conn_track_pin = false;
                        map.reuse_fd(handle.as_fd()).map_err(|e| {
                            anyhow!(
                                "failed to reuse already pinned {}: {e}",
                                udp_conn_track_path.display()
                            )
                        })?;
                    }
                }
                NAME_QUIC_CONN_TRACK => {
                    if let Ok(handle) = MapHandle::from_pinned_path(&self.quic_conn_track_path) {
                        map.reuse_fd(handle.as_fd()).map_err(|e| {
                            anyhow!(
                                "failed to reuse already pinned {}: {e}",
                                self.quic_conn_track_path.display()
                            )
                        })?;
                        self.quic_conn_track_handle = Some(handle);
                    }
                }
                NAME_SOCKET_MAP => {
                    if let Ok(handle) = MapHandle::from_pinned_path(&self.socket_map_path) {
                        map.reuse_fd(handle.as_fd()).map_err(|e| {
                            anyhow!(
                                "failed to reuse already pinned {}: {e}",
                                self.socket_map_path.display()
                            )
                        })?;
                        self.socket_map_handle = Some(handle);
                    }
                }
                NAME_PROC_MAP => {
                    if let Ok(handle) = MapHandle::from_pinned_path(&proc_map_path) {
                        map.reuse_fd(handle.as_fd()).map_err(|e| {
                            anyhow!(
                                "failed to reuse already pinned {}: {e}",
                                proc_map_path.display()
                            )
                        })?;
                        self.proc_map_handle = Some(handle);
                    }
                }
                NAME_OBJECT_RODATA => {}
                _ => panic!("encountered unexpected map: `{name}`"),
            }
        }

        let mut object = open_object
            .load()
            .map_err(|e| anyhow!("failed to load object: {e}"))?;

        for mut map in object.maps_mut() {
            let name = map
                .name()
                .to_str()
                .ok_or_else(|| anyhow!("invalid map name"))?;
            match name {
                NAME_UDP_CONN_TRACK => {
                    if udp_conn_track_pin {
                        map.pin(&udp_conn_track_path)
                            .map_err(|e| anyhow!("failed to pin udp_conn_track map: {e}"))?;
                    }
                }
                NAME_QUIC_CONN_TRACK => {
                    if self.quic_conn_track_handle.is_none() {
                        map.pin(&self.quic_conn_track_path)
                            .map_err(|e| anyhow!("failed to pin quic_conn_track map: {e}"))?;
                        let handle = MapHandle::from_pinned_path(&self.quic_conn_track_path)
                            .map_err(|e| {
                                anyhow!(
                                    "failed to open quic_conn_track map {}: {e}",
                                    self.quic_conn_track_path.display()
                                )
                            })?;
                        self.quic_conn_track_handle = Some(handle);
                    }
                }
                NAME_SOCKET_MAP => {
                    if self.socket_map_handle.is_none() {
                        map.pin(&self.socket_map_path)
                            .map_err(|e| anyhow!("failed to pin socket_map map: {e}"))?;
                        let handle =
                            MapHandle::from_pinned_path(&self.socket_map_path).map_err(|e| {
                                anyhow!(
                                    "failed to open socket_map {}: {e}",
                                    self.socket_map_path.display()
                                )
                            })?;
                        self.socket_map_handle = Some(handle);
                    }
                }
                NAME_PROC_MAP => {
                    if self.proc_map_handle.is_none() {
                        map.pin(&proc_map_path)
                            .map_err(|e| anyhow!("failed to pin proc_map map: {e}"))?;
                        let handle = MapHandle::from_pinned_path(&proc_map_path).map_err(|e| {
                            anyhow!("failed to open proc map {}: {e}", proc_map_path.display())
                        })?;
                        self.proc_map_handle = Some(handle);
                    }
                }
                NAME_OBJECT_RODATA => {}
                _ => panic!("encountered unexpected map: `{name}`"),
            }
        }

        self.register_sockets()?;
        self.register_proc()?;

        for prog in object.progs_mut() {
            let Some(name) = prog.name().to_str() else {
                continue;
            };
            match name {
                NAME_PROGRAM => {
                    if let Some((socket, _cookie)) = self.sockets.first() {
                        socket.attach_reuseport_ebpf(&prog.as_fd())?;
                    }
                }
                _ => panic!("encountered unexpected program: `{name}`"),
            }
        }

        Ok(())
    }

    fn register_sockets(&mut self) -> anyhow::Result<()> {
        let Some(cookie_handle) = &self.quic_conn_track_handle else {
            return Err(anyhow!("no quic_conn_track handle set"));
        };
        let Some(socket_handle) = &self.socket_map_handle else {
            return Err(anyhow!("no socket_map handle set"));
        };
        for (i, (socket, cookie)) in self.sockets.iter().enumerate() {
            let socket_id = SocketId {
                pid: self.pid,
                generation: self.generation,
                worker: i as u16,
            };
            cookie_handle
                .update(cookie.as_bytes(), socket_id.as_bytes(), MapFlags::ANY)
                .map_err(|e| {
                    anyhow!("failed to add #{i} socket {socket} to quic_conn_track map: {e}")
                })?;
            let socket_fd = socket.as_ebpf_fd();
            socket_handle
                .update(
                    socket_id.as_bytes(),
                    socket_fd.as_bytes(),
                    MapFlags::NO_EXIST,
                )
                .map_err(|e| anyhow!("failed to add #{i} socket {socket} to socket map: {e}"))?;
        }
        Ok(())
    }

    fn register_proc(&mut self) -> anyhow::Result<()> {
        let Some(handle) = &self.proc_map_handle else {
            return Err(anyhow!("no proc_map handle set"));
        };
        let key = ProcMapKey {
            pid: self.pid,
            generation: self.generation,
            padding: 0,
        };
        let value = ProcMapValue {
            invalid: 0,
            count: self.sockets.len() as u16,
            padding: 0,
        };
        handle
            .update(key.as_bytes(), value.as_bytes(), MapFlags::NO_EXIST)
            .map_err(|e| anyhow!("failed to add current proc to proc map: {e}"))?;
        Ok(())
    }

    pub fn unregister_proc(&mut self) {
        let Some(handle) = self.proc_map_handle.take() else {
            return;
        };
        let key = ProcMapKey {
            pid: self.pid,
            generation: self.generation,
            padding: 0,
        };
        let _ = handle.delete(key.as_bytes());
    }

    pub fn unregister_sockets(&mut self) {
        let Some(cookie_handle) = &self.quic_conn_track_handle else {
            return;
        };
        let Some(socket_map_handle) = self.socket_map_handle.take() else {
            return;
        };
        for (i, (_socket, cookie)) in self.sockets.iter().enumerate() {
            let key = SocketId {
                pid: self.pid,
                generation: self.generation,
                worker: i as u16,
            };
            let _ = cookie_handle.delete(cookie.as_bytes());
            let _ = socket_map_handle.delete(key.as_bytes());
        }
    }
}

impl Drop for QuicSocketSelector {
    fn drop(&mut self) {
        self.unregister_proc();
        // The cookies and sockets should be dropped in guard
    }
}
