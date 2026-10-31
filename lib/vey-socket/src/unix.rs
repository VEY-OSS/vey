/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::ops::Deref;
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::RawSocket;

static NEXT_CLIENT_ID: AtomicU64 = AtomicU64::new(1);

/// A connected std datagram client. Dropping it removes the client socket path.
pub struct StdConnectedDatagram {
    socket: Option<UnixDatagram>,
    path: PathBuf,
}

impl StdConnectedDatagram {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn into_tokio(mut self) -> io::Result<ConnectedDatagram> {
        let socket = self
            .socket
            .take()
            .expect("connected datagram socket is present");
        let path = std::mem::take(&mut self.path);
        match tokio::net::UnixDatagram::from_std(socket) {
            Ok(socket) => Ok(ConnectedDatagram { socket, path }),
            Err(e) => {
                let _ = std::fs::remove_file(&path);
                Err(e)
            }
        }
    }
}

impl Deref for StdConnectedDatagram {
    type Target = UnixDatagram;

    fn deref(&self) -> &Self::Target {
        self.socket
            .as_ref()
            .expect("connected datagram socket is present")
    }
}

impl Drop for StdConnectedDatagram {
    fn drop(&mut self) {
        unlink_client_path(&self.path);
    }
}

/// A connected Tokio datagram client. Dropping it removes the client socket path.
pub struct ConnectedDatagram {
    socket: tokio::net::UnixDatagram,
    path: PathBuf,
}

impl Deref for ConnectedDatagram {
    type Target = tokio::net::UnixDatagram;

    fn deref(&self) -> &Self::Target {
        &self.socket
    }
}

impl Drop for ConnectedDatagram {
    fn drop(&mut self) {
        unlink_client_path(&self.path);
    }
}

fn unlink_client_path(path: &Path) {
    if !path.as_os_str().is_empty() {
        let _ = std::fs::remove_file(path);
    }
}

/// Bind a pathname datagram client in the temp directory and connect it to `peer`.
///
/// The socket is nonblocking. `send_buf_size` sets `SO_SNDBUF` when present.
/// `name_prefix` is a single path component used in the client file name.
/// Dropping the returned value removes that file.
pub fn new_std_connected_datagram(
    peer: &Path,
    name_prefix: &str,
    send_buf_size: Option<usize>,
) -> io::Result<StdConnectedDatagram> {
    if name_prefix.is_empty()
        || name_prefix.contains(['/', '\\', '\0'])
        || name_prefix == "."
        || name_prefix == ".."
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unix client socket name prefix must be a single path component",
        ));
    }

    let id = NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("{name_prefix}-{}-{id}.sock", std::process::id()));
    let socket = UnixDatagram::bind(&path)?;
    let socket = StdConnectedDatagram {
        socket: Some(socket),
        path,
    };
    connect_client(&socket, peer, send_buf_size)?;
    Ok(socket)
}

fn connect_client(
    socket: &UnixDatagram,
    peer: &Path,
    send_buf_size: Option<usize>,
) -> io::Result<()> {
    if let Some(size) = send_buf_size {
        RawSocket::from(socket).set_send_buffer_size(size)?;
    }
    socket.connect(peer)?;
    socket.set_nonblocking(true)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefix(label: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("vey-socket-{label}-{}-{nanos}", std::process::id())
    }

    fn matching_paths(prefix: &str) -> Vec<PathBuf> {
        let dir = std::env::temp_dir();
        let mut paths = Vec::new();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return paths;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name.starts_with(prefix) {
                paths.push(dir.join(name));
            }
        }
        paths
    }

    #[test]
    fn connected_client_exchanges_with_the_peer() {
        let name = prefix("ok");
        let server_path = std::env::temp_dir().join(format!("{name}-server.sock"));
        let _ = std::fs::remove_file(&server_path);
        let server = UnixDatagram::bind(&server_path).unwrap();

        let client = new_std_connected_datagram(&server_path, &name, None).unwrap();
        let client_path = client.path().to_path_buf();
        assert_eq!(
            client.local_addr().unwrap().as_pathname(),
            Some(client_path.as_path())
        );

        client.send(b"ping").unwrap();
        let mut buf = [0u8; 8];
        let (n, addr) = server.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"ping");
        server
            .send_to(b"pong", addr.as_pathname().unwrap())
            .unwrap();
        let n = client.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"pong");

        drop(client);
        assert!(!client_path.exists());
        drop(server);
        let _ = std::fs::remove_file(&server_path);
    }

    #[test]
    fn missing_peer_removes_the_client_path() {
        let name = prefix("miss");
        let peer = std::env::temp_dir().join(format!("{name}-server.sock"));
        let _ = std::fs::remove_file(&peer);
        assert!(new_std_connected_datagram(&peer, &name, None).is_err());
        assert!(matching_paths(&name).is_empty());
    }

    #[test]
    fn invalid_name_prefix_is_rejected() {
        let peer = std::env::temp_dir().join("vey-socket-unused.sock");
        assert!(new_std_connected_datagram(&peer, "", Default::default()).is_err());
        assert!(new_std_connected_datagram(&peer, "a/b", Default::default()).is_err());
        assert!(new_std_connected_datagram(&peer, "..", Default::default()).is_err());
    }
}
