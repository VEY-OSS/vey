/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fs;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::num::NonZeroU32;
use std::os::fd::AsRawFd;
use std::thread;
use std::time::Duration;

use anyhow::Context;

use vey_reuseport::udp::UdpSocketSelector;
use vey_socket::RawSocket;

fn create_reuseport_udp_socket(port: u16) -> io::Result<UdpSocket> {
    let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None)?;
    socket.set_reuse_address(true)?;
    socket.set_reuse_port(true)?;
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    socket.bind(&addr.into())?;
    socket.set_nonblocking(true)?;
    Ok(socket.into())
}

fn try_recv(socket: &UdpSocket) -> Option<String> {
    let mut buf = [0u8; 1024];
    match socket.recv(&mut buf) {
        Ok(ret) => Some(String::from_utf8_lossy(&buf[..ret]).into_owned()),
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => None,
        Err(_) => None,
    }
}

fn main() -> anyhow::Result<()> {
    println!("==================================================");
    println!("   UdpSocketSelector Hot-Upgrade & Fallback Test  ");
    println!("==================================================");

    let max_entries = NonZeroU32::new(1024).unwrap();

    // 1. Check root privileges
    if unsafe { libc::getuid() } != 0 {
        println!("[ERROR] This test case must be run as root to load eBPF programs and pin maps.");
        println!("Please run with: sudo cargo run --bin udp_test");
        std::process::exit(1);
    }

    // 2. Setup Generation 1
    println!("\n--- [Phase 1: Setup Generation 1] ---");
    let s1_gen1 = create_reuseport_udp_socket(0).context("failed to create Gen 1 socket 1")?;
    let port = s1_gen1
        .local_addr()
        .context("failed to get Gen 1 socket 1 address")?
        .port();
    let s2_gen1 = create_reuseport_udp_socket(port).context("failed to create Gen 1 socket 2")?;
    println!(
        "[Gen 1] Sockets created: FD {}, FD {} on port {}",
        s1_gen1.as_raw_fd(),
        s2_gen1.as_raw_fd(),
        port
    );

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut selector_gen1 = UdpSocketSelector::new(1234, 1, addr, max_entries)
        .context("failed to create Gen 1 selector")?;
    let _s1_gen1_guard = selector_gen1.add_socket(RawSocket::from(&s1_gen1));
    let _s2_gen1_guard = selector_gen1.add_socket(RawSocket::from(&s2_gen1));

    selector_gen1
        .load_and_attach()
        .context("failed to load/attach Gen 1")?;
    println!(
        "[Gen 1] eBPF program attached. Pinned maps at: {}",
        selector_gen1.pin_dir().display()
    );

    // 3. Send traffic to Gen 1 to establish connection tracking
    println!("\n--- [Phase 2: Establish Tracked Connection] ---");
    let client1 = UdpSocket::bind("127.0.0.1:0").context("failed to bind client 1")?;
    client1
        .connect(format!("127.0.0.1:{port}"))
        .context("failed to connect client 1")?;
    println!("[Client 1] Bound to: {}", client1.local_addr()?);

    client1
        .send(b"P1-initial")
        .context("failed to send initial packet")?;
    thread::sleep(Duration::from_millis(50));

    let recv_gen1_s1 = try_recv(&s1_gen1);
    let recv_gen1_s2 = try_recv(&s2_gen1);

    let (tracked_socket, other_gen1_socket, tracked_name) = if let Some(data) = recv_gen1_s1 {
        println!(
            "[Gen 1] Socket 1 (FD {}) received: {data:?}",
            s1_gen1.as_raw_fd(),
        );
        (&s1_gen1, &s2_gen1, "Socket 1")
    } else if let Some(data) = recv_gen1_s2 {
        println!(
            "[Gen 1] Socket 2 (FD {}) received: {data:?}",
            s2_gen1.as_raw_fd(),
        );
        (&s2_gen1, &s1_gen1, "Socket 2")
    } else {
        anyhow::bail!("[ERROR] No Gen 1 socket received the packet!");
    };

    // 4. Setup Generation 2 (hot-upgrade)
    println!("\n--- [Phase 3: Setup Generation 2 (Hot-Upgrade)] ---");
    let s1_gen2 = create_reuseport_udp_socket(port).context("failed to create Gen 2 socket 1")?;
    let s2_gen2 = create_reuseport_udp_socket(port).context("failed to create Gen 2 socket 2")?;
    println!(
        "[Gen 2] Sockets created: FD {}, FD {}",
        s1_gen2.as_raw_fd(),
        s2_gen2.as_raw_fd()
    );

    let mut selector_gen2 = UdpSocketSelector::new(1234, 2, addr, max_entries)
        .context("failed to create Gen 2 selector")?;
    let _s1_gen2_guard = selector_gen2.add_socket(RawSocket::from(&s1_gen2));
    let _s2_gen2_guard = selector_gen2.add_socket(RawSocket::from(&s2_gen2));

    selector_gen2
        .load_and_attach()
        .context("failed to load/attach Gen 2")?;
    println!("[Gen 2] eBPF program attached (replaces Gen 1 on the reuseport group).");

    // 5. Send packets from old client (should route to the same Gen 1 socket via conn_track)
    println!("\n--- [Phase 4: Verify Old Tracked Connection Routing] ---");
    println!(
        "[Client 1] Sending another packet (should route to Gen 1 {})...",
        tracked_name
    );
    client1
        .send(b"P1-tracked")
        .context("failed to send tracked packet")?;
    thread::sleep(Duration::from_millis(50));

    // Verify who received it
    let recv_tracked = try_recv(tracked_socket);
    let recv_other_gen1 = try_recv(other_gen1_socket);
    let recv_gen2_s1 = try_recv(&s1_gen2);
    let recv_gen2_s2 = try_recv(&s2_gen2);

    if let Some(msg) = recv_tracked {
        println!(
            "[SUCCESS] Old tracked Gen 1 {} (FD {}) successfully received: {msg:?}",
            tracked_name,
            tracked_socket.as_raw_fd()
        );
    } else {
        println!("[ERROR] Old tracked Gen 1 socket did not receive the packet!");
        println!("  -> Other Gen 1: {:?}", recv_other_gen1);
        println!("  -> Gen 2 Socket 1: {:?}", recv_gen2_s1);
        println!("  -> Gen 2 Socket 2: {:?}", recv_gen2_s2);
        anyhow::bail!("Tracked connection steering failed after upgrade.");
    }

    // 6. Send packets from a new client (should route to a Gen 2 socket)
    println!("\n--- [Phase 5: Verify New Connection Routing] ---");
    let client2 = UdpSocket::bind("127.0.0.1:0").context("failed to bind client 2")?;
    client2
        .connect(format!("127.0.0.1:{port}"))
        .context("failed to connect client 2")?;
    println!(
        "[Client 2] Bound to: {} (new connection)",
        client2.local_addr()?
    );

    client2
        .send(b"P2-new")
        .context("failed to send new connection packet")?;
    thread::sleep(Duration::from_millis(50));

    let recv_gen2_s1 = try_recv(&s1_gen2);
    let recv_gen2_s2 = try_recv(&s2_gen2);
    let recv_gen1_s1 = try_recv(&s1_gen1);
    let recv_gen1_s2 = try_recv(&s2_gen1);

    if let Some(data) = recv_gen2_s1 {
        println!(
            "[SUCCESS] Gen 2 Socket 1 (FD {}) received the new connection packet: {data:?}",
            s1_gen2.as_raw_fd(),
        );
    } else if let Some(data) = recv_gen2_s2 {
        println!(
            "[SUCCESS] Gen 2 Socket 2 (FD {}) received the new connection packet: {data:?}",
            s2_gen2.as_raw_fd(),
        );
    } else {
        println!("[ERROR] No Gen 2 socket received the new connection packet!");
        println!("  -> Gen 1 Socket 1: {:?}", recv_gen1_s1);
        println!("  -> Gen 1 Socket 2: {:?}", recv_gen1_s2);
        anyhow::bail!("New connection steering failed to target Gen 2.");
    }

    // 7. Drop Gen 1 sockets and selector
    println!("\n--- [Phase 6: Drop Generation 1] ---");
    println!(
        "[Gen 1] Dropping Gen 1 sockets (FD {}, FD {}) and selector...",
        s1_gen1.as_raw_fd(),
        s2_gen1.as_raw_fd()
    );
    // Explicitly drop sockets to close the file descriptors
    drop(s1_gen1);
    drop(s2_gen1);
    drop(selector_gen1);
    thread::sleep(Duration::from_millis(50));

    // 8. Send traffic from client 1 again (should fall back to Gen 2 sockets)
    println!("\n--- [Phase 7: Verify Graceful Fallback for Tracked Connection] ---");
    println!(
        "[Client 1] Sending packet on the old connection (Gen 1 is gone, should route to Gen 2)..."
    );
    client1
        .send(b"P1-fallback")
        .context("failed to send fallback packet")?;
    thread::sleep(Duration::from_millis(50));

    let recv_gen2_s1 = try_recv(&s1_gen2);
    let recv_gen2_s2 = try_recv(&s2_gen2);

    if let Some(data) = recv_gen2_s1 {
        println!(
            "[SUCCESS] Gen 2 Socket 1 (FD {}) received fallback packet: {data:?}",
            s1_gen2.as_raw_fd(),
        );
    } else if let Some(data) = recv_gen2_s2 {
        println!(
            "[SUCCESS] Gen 2 Socket 2 (FD {}) received fallback packet: {data:?}",
            s2_gen2.as_raw_fd(),
        );
    } else {
        anyhow::bail!("[ERROR] Old connection packet was lost! Neither Gen 2 socket received it.");
    }

    // 9. Cleanup Gen 2
    println!("\n--- [Phase 8: Cleanup] ---");
    let pin_dir = selector_gen2.pin_dir().to_path_buf();
    drop(s1_gen2);
    drop(s2_gen2);
    drop(selector_gen2);

    let _ = fs::remove_file(pin_dir.join("conn_track"));
    let _ = fs::remove_file(pin_dir.join("proc_map"));
    let _ = fs::remove_file(pin_dir.join("socket_map"));
    let _ = fs::remove_dir(&pin_dir);
    if let Some(parent) = pin_dir.parent() {
        let _ = fs::remove_dir(parent);
        if let Some(grandparent) = parent.parent() {
            let _ = fs::remove_dir(grandparent);
        }
    }
    println!("[Cleanup] Gen 2 resources and bpffs directories removed.");

    println!("\n==================================================");
    println!("   All Phases Passed Successfully! (Hot-Upgrade OK) ");
    println!("==================================================");
    Ok(())
}
