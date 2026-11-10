/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::net::SocketAddr;
#[cfg(unix)]
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, anyhow};
use clap::{Arg, ArgGroup, Command};
use tokio::net::UdpSocket;
#[cfg(unix)]
use tokio::net::UnixDatagram;

use vey_cert_agent::{CertificateReply, Request};
use vey_types::net::Host;

const ARG_UDP: &str = "udp";
const ARG_UNIX: &str = "unix";
const ARG_TIMEOUT: &str = "timeout";
const ARG_HOST: &str = "host";

const DEFAULT_UDP: &str = "127.0.0.1:2999";

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    match run().await {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::from(1),
        Err(e) => {
            eprintln!("Error: {e:#}");
            std::process::ExitCode::from(1)
        }
    }
}

async fn run() -> anyhow::Result<bool> {
    let mut command = Command::new(env!("CARGO_PKG_NAME"))
        .about("Request one certificate from a vey-dcgen UDP or Unix datagram server")
        .arg(
            Arg::new(ARG_UDP)
                .short('u')
                .long("udp")
                .value_name("ADDR")
                .help(format!("UDP server address [default: {DEFAULT_UDP}]")),
        )
        .arg(
            Arg::new(ARG_TIMEOUT)
                .short('t')
                .long("timeout")
                .value_name("DURATION")
                .default_value("2s")
                .help("How long to wait for the certificate (e.g. 500ms, 2s)"),
        )
        .arg(
            Arg::new(ARG_HOST)
                .required(true)
                .value_name("HOST")
                .help("Hostname to issue a certificate for"),
        );
    #[cfg(unix)]
    {
        command = command
            .arg(
                Arg::new(ARG_UNIX)
                    .short('U')
                    .long("unix")
                    .value_name("PATH")
                    .help("Unix datagram socket path"),
            )
            .group(
                ArgGroup::new("server")
                    .args([ARG_UDP, ARG_UNIX])
                    .multiple(false),
            );
    }
    let args = command.get_matches();

    let timeout =
        parse_duration(args.get_one::<String>(ARG_TIMEOUT).unwrap()).context("invalid timeout")?;
    let host_arg = args.get_one::<String>(ARG_HOST).unwrap();
    let host = host_arg
        .parse::<Host>()
        .with_context(|| format!("invalid host {host_arg}"))?;

    #[cfg(unix)]
    if let Some(path) = args.get_one::<String>(ARG_UNIX) {
        let client = BoundUnixClient::connect(Path::new(path))?;
        return query_and_print(&*client, &host, timeout).await;
    }

    let udp = args
        .get_one::<String>(ARG_UDP)
        .map(String::as_str)
        .unwrap_or(DEFAULT_UDP);
    let server: SocketAddr = udp
        .parse()
        .with_context(|| format!("invalid UDP address {udp}"))?;
    let bind = if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let socket = UdpSocket::bind(bind)
        .await
        .with_context(|| format!("failed to bind {bind}"))?;
    socket
        .connect(server)
        .await
        .with_context(|| format!("failed to connect to {server}"))?;
    query_and_print(&socket, &host, timeout).await
}

async fn query_and_print(
    socket: &impl QuerySocket,
    host: &Host,
    timeout: Duration,
) -> anyhow::Result<bool> {
    let packet = Request::encode_host(host)?;
    socket
        .send(&packet)
        .await
        .with_context(|| format!("failed to send query for {host}"))?;

    let mut buf = vec![0u8; 65536];
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let rest = deadline.saturating_duration_since(tokio::time::Instant::now());
        if rest.is_zero() {
            break;
        }
        let n = match tokio::time::timeout(rest, socket.recv(&mut buf)).await {
            Ok(Ok(n)) => n,
            Ok(Err(e)) => return Err(e).context("failed to receive a response"),
            Err(_) => break,
        };
        let Ok(reply) = CertificateReply::decode(&buf[..n]) else {
            continue;
        };
        print_reply(&reply);
        return Ok(true);
    }

    println!("host: {host}");
    println!("not found");
    Ok(false)
}

fn print_reply(reply: &CertificateReply) {
    println!("ttl: {}", reply.ttl);
    print!("{}", reply.certificates);
    print!("{}", reply.private_key);
}

#[cfg(unix)]
struct BoundUnixClient {
    socket: UnixDatagram,
    path: PathBuf,
}

#[cfg(unix)]
impl BoundUnixClient {
    fn connect(server: &Path) -> anyhow::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "vey-dcgen-get-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let socket = UnixDatagram::bind(&path)
            .with_context(|| format!("failed to bind a client socket at {}", path.display()))?;
        if let Err(e) = socket.connect(server) {
            let _ = std::fs::remove_file(&path);
            return Err(e).with_context(|| format!("failed to connect to {}", server.display()));
        }
        Ok(Self { socket, path })
    }
}

#[cfg(unix)]
impl std::ops::Deref for BoundUnixClient {
    type Target = UnixDatagram;

    fn deref(&self) -> &Self::Target {
        &self.socket
    }
}

#[cfg(unix)]
impl Drop for BoundUnixClient {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

trait QuerySocket {
    async fn send(&self, packet: &[u8]) -> io::Result<usize>;
    async fn recv(&self, buf: &mut [u8]) -> io::Result<usize>;
}

impl QuerySocket for UdpSocket {
    async fn send(&self, packet: &[u8]) -> io::Result<usize> {
        UdpSocket::send(self, packet).await
    }

    async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        UdpSocket::recv(self, buf).await
    }
}

#[cfg(unix)]
impl QuerySocket for UnixDatagram {
    async fn send(&self, packet: &[u8]) -> io::Result<usize> {
        UnixDatagram::send(self, packet).await
    }

    async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        UnixDatagram::recv(self, buf).await
    }
}

fn parse_duration(text: &str) -> anyhow::Result<Duration> {
    if let Some(ms) = text.strip_suffix("ms") {
        let n: u64 = ms
            .parse()
            .with_context(|| format!("invalid millisecond duration {text}"))?;
        return Ok(Duration::from_millis(n));
    }
    let seconds = text.strip_suffix('s').unwrap_or(text);
    let n: f64 = seconds
        .parse()
        .with_context(|| format!("invalid duration {text}"))?;
    if !n.is_finite() || n < 0.0 {
        return Err(anyhow!("invalid duration {text}"));
    }
    Ok(Duration::from_secs_f64(n))
}
