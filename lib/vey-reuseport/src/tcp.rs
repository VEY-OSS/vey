/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, SocketAddr};
use std::os::fd::AsFd;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::{fs, ptr};

use anyhow::anyhow;
use libbpf_rs::{AsRawLibbpf, MapCore, MapFlags, MapHandle, OpenObject};
use zerocopy::IntoBytes;

use super::{ProcMapKey, ProcMapValue, ReadOnlyData, SocketId};

const NAME_OBJECT: &str = "tcp_bpf";
const NAME_OBJECT_RODATA: &str = "tcp_bpf.rodata";
const NAME_SOCKET_MAP: &str = "socket_map";
const NAME_PROC_MAP: &str = "proc_map";
const NAME_PROGRAM: &str = "tcp_select_reuseport";

#[unsafe(link_section = ".bpf.objs")]
static BPF_DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/tcp.bpf.o"));

pub struct TcpSocketSelector {
    pin_dir: PathBuf,
    pid: i32,
    generation: u16,
    sockets: Vec<RawFd>,
    proc_map_handle: Option<MapHandle>,
    socket_map_handle: Option<MapHandle>,
}

impl TcpSocketSelector {
    pub fn pin_dir(&self) -> &Path {
        &self.pin_dir
    }

    pub fn new(pid: i32, generation: u16, addr: SocketAddr) -> anyhow::Result<Self> {
        let ip = match addr.ip() {
            IpAddr::V4(ip) => ip.to_ipv6_compatible(), // IPv4 "." is not allowed in path
            IpAddr::V6(ip) => ip,
        };
        let dir = format!("/sys/fs/bpf/vey-reuseport/tcp/{ip}_{}", addr.port());
        let pin_dir = PathBuf::from(dir);

        fs::create_dir_all(&pin_dir)
            .map_err(|e| anyhow!("failed to create pin directory {}: {e}", pin_dir.display()))?;

        Ok(TcpSocketSelector {
            pin_dir,
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

    fn open_object(&mut self) -> anyhow::Result<OpenObject> {
        let mut builder = libbpf_rs::__internal_skel::ObjectSkeletonConfigBuilder::new(BPF_DATA);
        builder
            .name(NAME_OBJECT)
            .map(NAME_OBJECT_RODATA, true)
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

        let socket_map_path = self.pin_dir.join(NAME_SOCKET_MAP);
        let proc_map_path = self.pin_dir.join(NAME_PROC_MAP);

        for mut map in open_object.maps_mut() {
            let Some(name) = map.name().to_str() else {
                continue;
            };
            match name {
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
                    if let Some(fd) = self.sockets.first() {
                        let prog_fd = prog.as_fd().as_raw_fd();
                        super::attach_reuseport_ebpf(*fd, prog_fd)?;
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
            let value = *socket as u64;
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

impl Drop for TcpSocketSelector {
    fn drop(&mut self) {
        self.unregister_proc();
        self.unregister_sockets();
    }
}
