/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fs;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::os::fd::AsRawFd;
use std::thread;
use std::time::Duration;

use anyhow::Context;

use vey_reuseport::tcp::TcpSocketSelector;

fn create_reuseport_tcp_listener(port: u16) -> io::Result<TcpListener> {
    let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None)?;
    socket.set_reuse_address(true)?;
    socket.set_reuse_port(true)?;
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    socket.bind(&addr.into())?;
    socket.listen(128)?;
    socket.set_nonblocking(true)?;
    Ok(socket.into())
}

fn accept_from_listeners(
    listeners: &[(&TcpListener, &str)],
    timeout: Duration,
) -> anyhow::Result<(TcpStream, String)> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        for (listener, name) in listeners {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    stream.set_nonblocking(true)?;
                    return Ok((stream, name.to_string()));
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    anyhow::bail!("accept failed on {name}: {e}");
                }
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    anyhow::bail!("Timeout waiting for connection to be accepted");
}

fn send_and_recv_tcp(
    client: &mut TcpStream,
    server: &mut TcpStream,
    msg: &[u8],
) -> anyhow::Result<()> {
    use std::io::{Read, Write};
    client.write_all(msg).context("client write failed")?;
    thread::sleep(Duration::from_millis(50));
    let mut buf = [0u8; 1024];
    match server.read(&mut buf) {
        Ok(n) => {
            if &buf[..n] == msg {
                Ok(())
            } else {
                anyhow::bail!("data mismatch: expected {:?}, got {:?}", msg, &buf[..n]);
            }
        }
        Err(e) => {
            anyhow::bail!("server read failed: {e}");
        }
    }
}

fn main() -> anyhow::Result<()> {
    println!("==================================================");
    println!("   TcpSocketSelector Hot-Upgrade & Fallback Test  ");
    println!("==================================================");

    // 1. Check root privileges
    if unsafe { libc::getuid() } != 0 {
        println!("[ERROR] This test case must be run as root to load eBPF programs and pin maps.");
        println!("Please run with: sudo cargo run --bin tcp_reuseport_test");
        std::process::exit(1);
    }

    // 2. Setup Generation 1
    println!("\n--- [Phase 1: Setup Generation 1] ---");
    let s1_gen1 = create_reuseport_tcp_listener(0).context("failed to create Gen 1 socket 1")?;
    let port = s1_gen1
        .local_addr()
        .context("failed to get Gen 1 socket 1 address")?
        .port();
    let s2_gen1 = create_reuseport_tcp_listener(port).context("failed to create Gen 1 socket 2")?;
    println!(
        "[Gen 1] Sockets created: FD {}, FD {} on port {}",
        s1_gen1.as_raw_fd(),
        s2_gen1.as_raw_fd(),
        port
    );

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut selector_gen1 =
        TcpSocketSelector::new(1234, 1, addr).context("failed to create Gen 1 selector")?;
    selector_gen1.add_socket(s1_gen1.as_raw_fd());
    selector_gen1.add_socket(s2_gen1.as_raw_fd());

    selector_gen1
        .load_and_attach()
        .context("failed to load/attach Gen 1")?;
    println!(
        "[Gen 1] eBPF program attached. Pinned maps at: {}",
        selector_gen1.pin_dir().display()
    );

    // 3. Establish connection
    println!("\n--- [Phase 2: Establish Tracked Connection] ---");
    let mut client1 =
        TcpStream::connect(format!("127.0.0.1:{port}")).context("failed to connect client 1")?;
    client1.set_nonblocking(true)?;
    println!("[Client 1] Bound to: {}", client1.local_addr()?);

    let (mut server_stream1, tracked_name) = accept_from_listeners(
        &[(&s1_gen1, "Socket 1"), (&s2_gen1, "Socket 2")],
        Duration::from_secs(2),
    )
    .context("No Gen 1 socket accepted the connection!")?;

    println!("[Gen 1] {} accepted connection", tracked_name);

    // Verify initial data exchange
    send_and_recv_tcp(&mut client1, &mut server_stream1, b"P1-initial")
        .context("initial communication failed")?;
    println!("[Gen 1] Communication verified successfully on {tracked_name}.");

    // 4. Setup Generation 2 (hot-upgrade)
    println!("\n--- [Phase 3: Setup Generation 2 (Hot-Upgrade)] ---");
    let s1_gen2 = create_reuseport_tcp_listener(port).context("failed to create Gen 2 socket 1")?;
    let s2_gen2 = create_reuseport_tcp_listener(port).context("failed to create Gen 2 socket 2")?;
    println!(
        "[Gen 2] Sockets created: FD {}, FD {}",
        s1_gen2.as_raw_fd(),
        s2_gen2.as_raw_fd()
    );

    let mut selector_gen2 =
        TcpSocketSelector::new(1234, 2, addr).context("failed to create Gen 2 selector")?;
    selector_gen2.add_socket(s1_gen2.as_raw_fd());
    selector_gen2.add_socket(s2_gen2.as_raw_fd());

    selector_gen2
        .load_and_attach()
        .context("failed to load/attach Gen 2")?;
    println!("[Gen 2] eBPF program attached (replaces Gen 1 on the reuseport group).");

    // 5. Send packets from old client (should route to the same established server stream)
    println!("\n--- [Phase 4: Verify Old Established Connection Routing] ---");
    println!("[Client 1] Sending another packet (should route to established server stream)...");
    send_and_recv_tcp(&mut client1, &mut server_stream1, b"P1-tracked")
        .context("tracked communication failed")?;
    println!(
        "[SUCCESS] Old established Gen 1 {} successfully communicated: \"P1-tracked\"",
        tracked_name
    );

    // 6. Send packets from a new client (should route to a Gen 2 socket)
    println!("\n--- [Phase 5: Verify New Connection Routing] ---");
    let mut client2 =
        TcpStream::connect(format!("127.0.0.1:{port}")).context("failed to connect client 2")?;
    client2.set_nonblocking(true)?;
    println!(
        "[Client 2] Bound to: {} (new connection)",
        client2.local_addr()?
    );

    let (mut server_stream2, accepted_name) = accept_from_listeners(
        &[(&s1_gen2, "Gen 2 Socket 1"), (&s2_gen2, "Gen 2 Socket 2")],
        Duration::from_secs(2),
    )
    .context("New connection steering failed to target Gen 2.")?;

    println!("[SUCCESS] {} accepted the new connection", accepted_name);

    // Verify that Gen 1 sockets did not accept the new connection
    if s1_gen1.accept().is_ok() {
        anyhow::bail!("Gen 1 Socket 1 accepted new connection after upgrade!");
    }
    if s2_gen1.accept().is_ok() {
        anyhow::bail!("Gen 1 Socket 2 accepted new connection after upgrade!");
    }

    // Verify communication on Gen 2
    send_and_recv_tcp(&mut client2, &mut server_stream2, b"P2-new")
        .context("new connection communication failed")?;

    // 7. Drop Gen 2 (to test fallback to Gen 1)
    println!("\n--- [Phase 6: Drop Generation 2 to Test Fallback] ---");
    println!(
        "[Gen 2] Dropping Gen 2 sockets (FD {}, FD {}) and selector...",
        s1_gen2.as_raw_fd(),
        s2_gen2.as_raw_fd()
    );
    drop(s1_gen2);
    drop(s2_gen2);
    drop(selector_gen2);
    thread::sleep(Duration::from_millis(50));

    // 8. Connect client 3 (should fall back to Gen 1 sockets since Gen 2 is dropped)
    println!("\n--- [Phase 7: Verify Graceful Fallback to Generation 1] ---");
    let mut client3 =
        TcpStream::connect(format!("127.0.0.1:{port}")).context("failed to connect client 3")?;
    client3.set_nonblocking(true)?;
    println!(
        "[Client 3] Bound to: {} (fallback connection)",
        client3.local_addr()?
    );

    let (mut server_stream3, fallback_name) = accept_from_listeners(
        &[(&s1_gen1, "Gen 1 Socket 1"), (&s2_gen1, "Gen 1 Socket 2")],
        Duration::from_secs(2),
    )
    .context("Fallback failed! Neither Gen 1 socket accepted the connection.")?;

    println!(
        "[SUCCESS] {} accepted the fallback connection",
        fallback_name
    );

    // Verify communication on Gen 1 fallback
    send_and_recv_tcp(&mut client3, &mut server_stream3, b"P1-fallback")
        .context("fallback connection communication failed")?;

    // 9. Cleanup Gen 1
    println!("\n--- [Phase 8: Cleanup] ---");
    let pin_dir = selector_gen1.pin_dir().to_path_buf();
    drop(s1_gen1);
    drop(s2_gen1);
    drop(selector_gen1);

    let _ = fs::remove_file(pin_dir.join("proc_map"));
    let _ = fs::remove_file(pin_dir.join("socket_map"));
    let _ = fs::remove_dir(&pin_dir);
    if let Some(parent) = pin_dir.parent() {
        let _ = fs::remove_dir(parent);
        if let Some(grandparent) = parent.parent() {
            let _ = fs::remove_dir(grandparent);
        }
    }
    println!("[Cleanup] Gen 1 resources and bpffs directories removed.");

    println!("\n==================================================");
    println!("   All Phases Passed Successfully! (Hot-Upgrade OK) ");
    println!("==================================================");
    Ok(())
}
