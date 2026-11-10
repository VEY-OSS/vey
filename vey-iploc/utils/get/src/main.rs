/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::{HashMap, HashSet};
use std::io;
use std::net::{IpAddr, SocketAddr};
#[cfg(unix)]
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, anyhow};
use clap::{Arg, ArgGroup, Command};
use tokio::net::UdpSocket;
#[cfg(unix)]
use tokio::net::UnixDatagram;

use vey_geoip_types::IpLocation;
use vey_ip_locate::{Request, Response};

const ARG_UDP: &str = "udp";
const ARG_UNIX: &str = "unix";
const ARG_TIMEOUT: &str = "timeout";
const ARG_IP: &str = "ip";

const DEFAULT_UDP: &str = "127.0.0.1:2888";

struct Lookup {
    location: IpLocation,
    ttl: Option<u32>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    match run().await {
        Ok(0) => std::process::ExitCode::SUCCESS,
        Ok(_) => std::process::ExitCode::from(1),
        Err(e) => {
            eprintln!("Error: {e:#}");
            std::process::ExitCode::from(1)
        }
    }
}

async fn run() -> anyhow::Result<usize> {
    let mut command = Command::new(env!("CARGO_PKG_NAME"))
        .about("Look up IP locations from a vey-iploc UDP or Unix datagram server")
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
                .default_value("1s")
                .help("How long to wait for the whole batch (e.g. 500ms, 2s)"),
        )
        .arg(
            Arg::new(ARG_IP)
                .required(true)
                .num_args(1..)
                .value_name("IP")
                .help("One or more IP addresses"),
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
    let ips = args
        .get_many::<String>(ARG_IP)
        .unwrap()
        .map(|s| {
            s.parse::<IpAddr>()
                .with_context(|| format!("invalid IP address {s}"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    #[cfg(unix)]
    if let Some(path) = args.get_one::<String>(ARG_UNIX) {
        let client = BoundUnixClient::connect(Path::new(path))?;
        return query_and_print(&*client, &ips, timeout).await;
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
    query_and_print(&socket, &ips, timeout).await
}

async fn query_and_print(
    socket: &impl QuerySocket,
    ips: &[IpAddr],
    timeout: Duration,
) -> anyhow::Result<usize> {
    let found = lookup_all(socket, ips, timeout).await?;
    Ok(print_lookups(ips, &found))
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
            "vey-iploc-get-{}-{}.sock",
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

async fn lookup_all(
    socket: &impl QuerySocket,
    ips: &[IpAddr],
    timeout: Duration,
) -> anyhow::Result<HashMap<IpAddr, Lookup>> {
    let mut unique = Vec::new();
    let mut waiting = HashSet::new();
    for ip in ips {
        if waiting.insert(*ip) {
            unique.push(*ip);
        }
    }
    let found = Mutex::new(HashMap::<IpAddr, Lookup>::new());

    let send_fut = async {
        for ip in unique {
            let packet = Request::encode_new(ip)?;
            socket
                .send(&packet)
                .await
                .with_context(|| format!("failed to send query for {ip}"))?;
        }
        Ok(())
    };
    let recv_fut = async {
        let mut buf = vec![0u8; 65536];
        loop {
            if found.lock().unwrap().len() >= waiting.len() {
                break;
            }
            let n = socket
                .recv(&mut buf)
                .await
                .context("failed to receive a response")?;
            let Ok((Some(ip), Some(location), ttl)) = Response::decode(&buf[..n]) else {
                continue;
            };
            if waiting.contains(&ip) {
                found.lock().unwrap().insert(ip, Lookup { location, ttl });
            }
        }
        Ok(())
    };

    match tokio::time::timeout(timeout, async { tokio::try_join!(send_fut, recv_fut) }).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return Err(e),
        Err(_) => {}
    }
    Ok(found.into_inner().unwrap())
}

fn print_lookups(ips: &[IpAddr], found: &HashMap<IpAddr, Lookup>) -> usize {
    let mut missing = 0usize;
    for (i, ip) in ips.iter().enumerate() {
        if i > 0 {
            println!();
        }
        println!("ip: {ip}");
        if let Some(lookup) = found.get(ip) {
            print_lookup(lookup);
        } else {
            missing += 1;
            println!("not found");
        }
    }
    missing
}

fn print_lookup(lookup: &Lookup) {
    println!("network: {}", lookup.location.network_addr());
    if let Some(country) = lookup.location.country() {
        println!("country: {}", country.alpha2_code());
    }
    if let Some(continent) = lookup.location.continent() {
        println!("continent: {}", continent.code());
    }
    if let Some(asn) = lookup.location.network_asn() {
        println!("asn: {asn}");
    }
    if let Some(name) = lookup.location.isp_name() {
        println!("isp_name: {name}");
    }
    if let Some(domain) = lookup.location.isp_domain() {
        println!("isp_domain: {domain}");
    }
    if let Some(ttl) = lookup.ttl {
        println!("ttl: {ttl}");
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
