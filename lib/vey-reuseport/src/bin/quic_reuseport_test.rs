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

use vey_reuseport::quic::QuicSocketSelector;

fn create_reuseport_udp_socket(port: u16) -> io::Result<UdpSocket> {
    let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None)?;
    socket.set_reuse_address(true)?;
    socket.set_reuse_port(true)?;
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    socket.bind(&addr.into())?;
    socket.set_nonblocking(true)?;
    Ok(socket.into())
}

fn try_recv(socket: &UdpSocket) -> Option<Vec<u8>> {
    let mut buf = [0u8; 1024];
    match socket.recv(&mut buf) {
        Ok(ret) => Some(buf[..ret].to_vec()),
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => None,
        Err(_) => None,
    }
}

fn send_short_packet(client: &UdpSocket, cookie: u64) -> io::Result<()> {
    let mut payload = vec![0u8; 9];
    payload[0] = 0x00; // MSB clear = Short packet
    payload[1..9].copy_from_slice(&cookie.to_be_bytes());
    client.send(&payload)?;
    Ok(())
}

fn send_long_packet(client: &UdpSocket, data: &[u8]) -> io::Result<()> {
    let mut payload = vec![0u8; 1 + data.len()];
    payload[0] = 0x80; // MSB set = Long packet
    payload[1..].copy_from_slice(data);
    client.send(&payload)?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    println!("==================================================");
    println!("   QuicSocketSelector Hot-Upgrade & Fallback Test ");
    println!("==================================================");

    let max_entries = NonZeroU32::new(1024).unwrap();

    // 1. Check root privileges
    if unsafe { libc::getuid() } != 0 {
        println!("[ERROR] This test case must be run as root to load eBPF programs and pin maps.");
        println!("Please run with: sudo cargo run --bin quic_reuseport_test");
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
    let mut selector_gen1 = QuicSocketSelector::new(1234, 1, addr, max_entries)
        .context("failed to create Gen 1 selector")?;

    let cookie1 = 0x1122334455667788u64;
    let cookie2 = 0x99aabbccddeeff00u64;

    selector_gen1.add_socket(s1_gen1.as_raw_fd(), cookie1);
    selector_gen1.add_socket(s2_gen1.as_raw_fd(), cookie2);

    selector_gen1
        .load_and_attach()
        .context("failed to load/attach Gen 1")?;
    println!(
        "[Gen 1] eBPF program attached. Pinned maps at: {}",
        selector_gen1.pin_dir().display()
    );

    // 3. Setup client 1
    let client1 = UdpSocket::bind("127.0.0.1:0").context("failed to bind client 1")?;
    client1
        .connect(format!("127.0.0.1:{port}"))
        .context("failed to connect client 1")?;
    println!("[Client 1] Bound to: {}", client1.local_addr()?);

    // 4. Test Short Packet Routing in Gen 1
    println!("\n--- [Phase 2: Test Short Packet Routing in Gen 1] ---");
    println!("[Client 1] Sending short packet with cookie1...");
    send_short_packet(&client1, cookie1).context("failed to send short packet cookie1")?;
    thread::sleep(Duration::from_millis(50));

    let recv_gen1_s1 = try_recv(&s1_gen1);
    let recv_gen1_s2 = try_recv(&s2_gen1);

    if recv_gen1_s1.is_some() && recv_gen1_s2.is_none() {
        println!("[SUCCESS] Gen 1 Socket 1 received the packet for cookie1.");
    } else {
        println!(
            "[ERROR] Expected Gen 1 Socket 1 to receive packet, Gen 1: {:?}, Gen 2: {:?}",
            recv_gen1_s1, recv_gen1_s2
        );
        anyhow::bail!("Short packet routing for cookie1 failed.");
    }

    println!("[Client 1] Sending short packet with cookie2...");
    send_short_packet(&client1, cookie2).context("failed to send short packet cookie2")?;
    thread::sleep(Duration::from_millis(50));

    let recv_gen1_s1 = try_recv(&s1_gen1);
    let recv_gen1_s2 = try_recv(&s2_gen1);

    if recv_gen1_s2.is_some() && recv_gen1_s1.is_none() {
        println!("[SUCCESS] Gen 1 Socket 2 received the packet for cookie2.");
    } else {
        println!(
            "[ERROR] Expected Gen 1 Socket 2 to receive packet, Gen 1: {:?}, Gen 2: {:?}",
            recv_gen1_s1, recv_gen1_s2
        );
        anyhow::bail!("Short packet routing for cookie2 failed.");
    }

    // 5. Test Long Packet Routing and Session Affinity in Gen 1
    println!("\n--- [Phase 3: Test Long Packet Routing and Session Affinity in Gen 1] ---");
    println!("[Client 1] Sending initial long packet...");
    send_long_packet(&client1, b"initial").context("failed to send long packet")?;
    thread::sleep(Duration::from_millis(50));

    let recv_gen1_s1 = try_recv(&s1_gen1);
    let recv_gen1_s2 = try_recv(&s2_gen1);

    let (tracked_socket, other_gen1_socket, tracked_name) = if recv_gen1_s1.is_some() {
        println!(
            "[Gen 1] Socket 1 (FD {}) received the long packet.",
            s1_gen1.as_raw_fd()
        );
        (&s1_gen1, &s2_gen1, "Socket 1")
    } else if recv_gen1_s2.is_some() {
        println!(
            "[Gen 1] Socket 2 (FD {}) received the long packet.",
            s2_gen1.as_raw_fd()
        );
        (&s2_gen1, &s1_gen1, "Socket 2")
    } else {
        anyhow::bail!("[ERROR] No Gen 1 socket received the long packet!");
    };

    println!("[Client 1] Sending second long packet (verifying session affinity)...");
    send_long_packet(&client1, b"tracked").context("failed to send tracked long packet")?;
    thread::sleep(Duration::from_millis(50));

    let recv_tracked = try_recv(tracked_socket);
    let recv_other = try_recv(other_gen1_socket);

    if recv_tracked.is_some() && recv_other.is_none() {
        println!(
            "[SUCCESS] Tracked socket ({tracked_name}) successfully received the session packet."
        );
    } else {
        println!(
            "[ERROR] Session affinity failed. Tracked: {:?}, Other: {:?}",
            recv_tracked, recv_other
        );
        anyhow::bail!("Session affinity for long packets failed.");
    }

    // 6. Setup Generation 2 (Hot-Upgrade)
    println!("\n--- [Phase 4: Setup Generation 2 (Hot-Upgrade)] ---");
    let s1_gen2 = create_reuseport_udp_socket(port).context("failed to create Gen 2 socket 1")?;
    let s2_gen2 = create_reuseport_udp_socket(port).context("failed to create Gen 2 socket 2")?;
    println!(
        "[Gen 2] Sockets created: FD {}, FD {}",
        s1_gen2.as_raw_fd(),
        s2_gen2.as_raw_fd()
    );

    let mut selector_gen2 = QuicSocketSelector::new(1234, 2, addr, max_entries)
        .context("failed to create Gen 2 selector")?;

    let cookie3 = 0x1111222233334444u64;
    let cookie4 = 0x5555666677778888u64;

    selector_gen2.add_socket(s1_gen2.as_raw_fd(), cookie3);
    selector_gen2.add_socket(s2_gen2.as_raw_fd(), cookie4);

    selector_gen2
        .load_and_attach()
        .context("failed to load/attach Gen 2")?;
    println!("[Gen 2] eBPF program attached (replaces Gen 1 on the reuseport group).");

    // 7. Test Short Packet Routing in Gen 2 and Gen 1 (both should work)
    println!("\n--- [Phase 5: Test Short Packet Routing in Gen 2 and Gen 1] ---");
    println!("[Client 1] Sending short packet with cookie3 (Gen 2 Socket 1)...");
    send_short_packet(&client1, cookie3).context("failed to send short packet cookie3")?;
    thread::sleep(Duration::from_millis(50));

    let recv_gen2_s1 = try_recv(&s1_gen2);
    let recv_gen2_s2 = try_recv(&s2_gen2);
    if recv_gen2_s1.is_some() && recv_gen2_s2.is_none() {
        println!("[SUCCESS] Gen 2 Socket 1 received the packet for cookie3.");
    } else {
        anyhow::bail!("Short packet routing for cookie3 failed.");
    }

    println!("[Client 1] Sending short packet with cookie4 (Gen 2 Socket 2)...");
    send_short_packet(&client1, cookie4).context("failed to send short packet cookie4")?;
    thread::sleep(Duration::from_millis(50));

    let recv_gen2_s1 = try_recv(&s1_gen2);
    let recv_gen2_s2 = try_recv(&s2_gen2);
    if recv_gen2_s2.is_some() && recv_gen2_s1.is_none() {
        println!("[SUCCESS] Gen 2 Socket 2 received the packet for cookie4.");
    } else {
        anyhow::bail!("Short packet routing for cookie4 failed.");
    }

    println!("[Client 1] Sending short packet with cookie1 (should route to Gen 1)...");
    send_short_packet(&client1, cookie1)
        .context("failed to send short packet cookie1 post-upgrade")?;
    thread::sleep(Duration::from_millis(50));

    let recv_tracked = try_recv(tracked_socket);
    let recv_other = try_recv(other_gen1_socket);
    let recv_gen2_s1 = try_recv(&s1_gen2);
    let recv_gen2_s2 = try_recv(&s2_gen2);

    if recv_tracked.is_some()
        && recv_other.is_none()
        && recv_gen2_s1.is_none()
        && recv_gen2_s2.is_none()
    {
        println!("[SUCCESS] Gen 1 tracked socket received cookie1 post-upgrade.");
    } else {
        anyhow::bail!("Short packet routing for cookie1 failed post-upgrade.");
    }

    // 8. Test Long Packet Routing after Upgrade
    println!("\n--- [Phase 6: Test Long Packet Routing after Upgrade] ---");
    println!("[Client 1] Sending long packet (should route to Gen 1)...");
    send_long_packet(&client1, b"tracked-post-upgrade")
        .context("failed to send long packet post-upgrade")?;
    thread::sleep(Duration::from_millis(50));

    let recv_tracked = try_recv(tracked_socket);
    if recv_tracked.is_some() {
        println!("[SUCCESS] Gen 1 tracked socket successfully received the long packet.");
    } else {
        anyhow::bail!("Session affinity for long packet failed post-upgrade.");
    }

    println!("[Client 2] Sending new long packet (should route to Gen 2)...");
    let client2 = UdpSocket::bind("127.0.0.1:0").context("failed to bind client 2")?;
    client2
        .connect(format!("127.0.0.1:{port}"))
        .context("failed to connect client 2")?;
    send_long_packet(&client2, b"new-conn").context("failed to send new long packet")?;
    thread::sleep(Duration::from_millis(50));

    let recv_gen2_s1 = try_recv(&s1_gen2);
    let recv_gen2_s2 = try_recv(&s2_gen2);
    let recv_gen1_s1 = try_recv(&s1_gen1);
    let recv_gen1_s2 = try_recv(&s2_gen1);

    if recv_gen2_s1.is_some() || recv_gen2_s2.is_some() {
        println!(
            "[SUCCESS] Gen 2 received the new connection long packet: s1={:?}, s2={:?}.",
            recv_gen2_s1, recv_gen2_s2
        );
    } else {
        println!(
            "[ERROR] New long packet did not route to Gen 2. Gen 1: s1={:?}, s2={:?}. Gen 2: s1={:?}, s2={:?}",
            recv_gen1_s1, recv_gen1_s2, recv_gen2_s1, recv_gen2_s2
        );
        anyhow::bail!("New connection routing for long packet failed.");
    }

    // 9. Drop Generation 1
    println!("\n--- [Phase 7: Drop Generation 1] ---");
    drop(s1_gen1);
    drop(s2_gen1);
    drop(selector_gen1);
    thread::sleep(Duration::from_millis(50));

    // 10. Verify Graceful Fallback for Short Packet (dead cookie)
    println!("\n--- [Phase 8: Verify Graceful Fallback for Short Packet] ---");
    println!("[Client 1] Sending short packet with cookie1 (belonged to dropped Gen 1)...");
    send_short_packet(&client1, cookie1).context("failed to send short packet for fallback")?;
    thread::sleep(Duration::from_millis(50));

    // Since Gen 1 is dropped, the BPF should fail to route to Gen 1 Socket 1,
    // delete cookie1 from quic_conn_track, and route to Gen 2 via fallback (kernel default reuseport).
    let recv_gen2_s1 = try_recv(&s1_gen2);
    let recv_gen2_s2 = try_recv(&s2_gen2);

    if recv_gen2_s1.is_some() || recv_gen2_s2.is_some() {
        println!(
            "[SUCCESS] Short packet with dead cookie1 fell back to Gen 2: s1={:?}, s2={:?}.",
            recv_gen2_s1, recv_gen2_s2
        );
    } else {
        anyhow::bail!("Short packet fallback failed.");
    }

    // 11. Verify Graceful Fallback for Long Packet
    println!("\n--- [Phase 9: Verify Graceful Fallback for Long Packet] ---");
    println!("[Client 1] Sending long packet (belonged to dropped Gen 1)...");
    send_long_packet(&client1, b"fallback").context("failed to send long packet for fallback")?;
    thread::sleep(Duration::from_millis(50));

    let recv_gen2_s1 = try_recv(&s1_gen2);
    let recv_gen2_s2 = try_recv(&s2_gen2);

    if recv_gen2_s1.is_some() || recv_gen2_s2.is_some() {
        println!(
            "[SUCCESS] Long packet fell back to Gen 2: s1={:?}, s2={:?}.",
            recv_gen2_s1, recv_gen2_s2
        );
    } else {
        anyhow::bail!("Long packet fallback failed.");
    }

    // 12. Cleanup Gen 2
    println!("\n--- [Phase 10: Cleanup] ---");
    let pin_dir = selector_gen2.pin_dir().to_path_buf();
    drop(s1_gen2);
    drop(s2_gen2);
    drop(selector_gen2);

    let _ = fs::remove_file(pin_dir.join("udp_conn_track"));
    let _ = fs::remove_file(pin_dir.join("quic_conn_track"));
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
