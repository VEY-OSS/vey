/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::{fs, ptr};

use anyhow::anyhow;
use libbpf_rs::{AsRawLibbpf, MapCore, MapFlags, MapHandle, OpenObject};
use log::warn;
use zerocopy::IntoBytes;

use vey_socket::RawSocket;

use super::{ProcMapKey, ProcMapValue, ReadOnlyData, SocketId};

const NAME_OBJECT: &str = "udp_bpf";
const NAME_OBJECT_RODATA: &str = "udp_bpf.rodata";
const NAME_CONN_TRACK: &str = "conn_track";
const NAME_SOCKET_MAP: &str = "socket_map";
const NAME_PROC_MAP: &str = "proc_map";
const NAME_PROGRAM: &str = "udp_select_reuseport";

#[unsafe(link_section = ".bpf.objs")]
static BPF_DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/udp.bpf.o"));

pub struct UdpSocketSelector {
    pin_dir: PathBuf,
    conn_track_max_entries: NonZeroU32,
    pid: i32,
    generation: u16,
    sockets: Vec<RawSocket>,
    proc_map_handle: Option<MapHandle>,
    socket_map_handle: Option<MapHandle>,
}

impl UdpSocketSelector {
    pub fn pin_dir(&self) -> &Path {
        &self.pin_dir
    }

    pub fn new(
        pid: i32,
        generation: u16,
        addr: SocketAddr,
        conn_track_max_entries: NonZeroU32,
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

    pub fn add_socket(&mut self, socket: RawSocket) {
        self.sockets.push(socket);
    }

    fn open_object(&mut self) -> anyhow::Result<OpenObject> {
        let mut builder = libbpf_rs::__internal_skel::ObjectSkeletonConfigBuilder::new(BPF_DATA);
        builder
            .name(NAME_OBJECT)
            .map(NAME_OBJECT_RODATA, true)
            .map(NAME_CONN_TRACK, false)
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

        let mut conn_track_pin = true;
        let conn_track_path = self.pin_dir.join(NAME_CONN_TRACK);
        let socket_map_path = self.pin_dir.join(NAME_SOCKET_MAP);
        let proc_map_path = self.pin_dir.join(NAME_PROC_MAP);

        for mut map in open_object.maps_mut() {
            let Some(name) = map.name().to_str() else {
                continue;
            };
            match name {
                NAME_CONN_TRACK => {
                    let max_entries = self.conn_track_max_entries.get();
                    if let Ok(handle) = MapHandle::from_pinned_path(&conn_track_path) {
                        if handle.max_entries() != max_entries {
                            warn!(
                                "udp conn_track map {} already pinned with max entries {}, delete it first if you want to set max entries to {}",
                                conn_track_path.display(),
                                handle.max_entries(),
                                self.conn_track_max_entries
                            );
                        }
                        conn_track_pin = false;
                        map.reuse_fd(handle.as_fd()).map_err(|e| {
                            anyhow!(
                                "failed to reuse already pinned {}: {e}",
                                conn_track_path.display()
                            )
                        })?;
                    } else {
                        map.set_max_entries(max_entries).map_err(|e| {
                            anyhow!("failed to set max entries for conn_track map: {e}")
                        })?;
                    }
                }
                NAME_SOCKET_MAP => {
                    if let Ok(handle) = MapHandle::from_pinned_path(&socket_map_path) {
                        map.reuse_fd(handle.as_fd()).map_err(|e| {
                            anyhow!(
                                "failed to reuse already pinned {}: {e}",
                                socket_map_path.display()
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
                NAME_CONN_TRACK => {
                    if conn_track_pin {
                        map.pin(&conn_track_path)
                            .map_err(|e| anyhow!("failed to pin conn_track map: {e}"))?;
                    }
                }
                NAME_SOCKET_MAP => {
                    if self.socket_map_handle.is_none() {
                        map.pin(&socket_map_path)
                            .map_err(|e| anyhow!("failed to pin socket_map map: {e}"))?;
                        let handle =
                            MapHandle::from_pinned_path(&socket_map_path).map_err(|e| {
                                anyhow!(
                                    "failed to open socket_map {}: {e}",
                                    socket_map_path.display()
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
                    if let Some(socket) = self.sockets.first() {
                        socket.attach_reuseport_ebpf(&prog.as_fd())?;
                    }
                }
                _ => panic!("encountered unexpected program: `{name}`"),
            }
        }

        Ok(())
    }

    fn register_sockets(&mut self) -> anyhow::Result<()> {
        let Some(handle) = &self.socket_map_handle else {
            return Err(anyhow!("no socket_map handle set"));
        };
        for (i, socket) in self.sockets.iter().enumerate() {
            let key = SocketId {
                pid: self.pid,
                generation: self.generation,
                worker: i as u16,
            };
            let value = socket.as_ebpf_fd();
            handle
                .update(key.as_bytes(), value.as_bytes(), MapFlags::NO_EXIST)
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
        let Some(handle) = self.socket_map_handle.take() else {
            return;
        };
        for i in 0..self.sockets.len() {
            let key = SocketId {
                pid: self.pid,
                generation: self.generation,
                worker: i as u16,
            };
            let _ = handle.delete(key.as_bytes());
        }
    }
}

impl Drop for UdpSocketSelector {
    fn drop(&mut self) {
        self.unregister_proc();
        self.unregister_sockets();
    }
}
