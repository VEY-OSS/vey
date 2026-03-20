[![docs](https://readthedocs.org/projects/vey-proxy/badge)](https://vey.readthedocs.io/projects/proxy/)

# VEY Proxy

`vey-proxy` is an enterprise-oriented forward proxy with built-in support for TCP streaming, TLS streaming,
transparent proxying, and basic reverse proxying.

## User Guide

[中文](UserGuide.zh_CN.md) | [English](UserGuide.en_US.md)

## Building

Set up the build environment first by following [dev-setup](../doc/dev-setup.md).

Build debug binaries:

```shell
cargo build -p vey-proxy -p vey-proxy-ctl
```

Build release binaries:

```shell
cargo build --profile release-lto -p vey-proxy -p vey-proxy-ctl
```

If you want to build binary packages or container images, see
[Build and Package](../doc/build_and_package.md).

## Documentation

The Sphinx-generated reference documentation is available on
[Read the Docs](https://vey.readthedocs.io/projects/proxy/en/latest/). It covers configuration formats, log formats,
metrics, protocol definitions, and related reference material.

## Examples

Example configurations are available in [examples](examples).

## Feature Overview

### Servers

Servers accept and process client connections. Different server types are available for different deployment patterns.

Common capabilities include:

- Ingress network filtering, target host filtering, and target port filtering
- Socket speed limits
- Request rate limiting and idle detection
- Protocol inspection, TLS/TLCP interception, and ICAP adaptation
- Extensive TCP and UDP socket configuration
- Rustls-based TLS server support
- OpenSSL / BoringSSL / AWS-LC / Tongsuo based TLS server and client support
- Tongsuo-based TLCP server and client support (`GB/T 38636-2020`)

#### Forward Proxy Servers

- HTTP(S) Proxy
    - TLS / mTLS
    - HTTP forward, HTTPS forward, HTTP CONNECT, and FTP over HTTP
    - easy-proxy and `masque/http` Well-Known URI support
    - Basic user authentication
    - Port hiding

- SOCKS Proxy
    - SOCKS4 TCP CONNECT, SOCKS5 TCP CONNECT, and SOCKS5 UDP ASSOCIATE
    - Basic user authentication
    - Client-side UDP IP binding, IP mapping, and ranged ports

#### Transparent Proxy Servers

- SNI Proxy
    - Multiple protocol detection: TLS SNI extension and HTTP Host header
    - Host redirection and host ACLs
    - Fact-based user authentication

- TCP TPROXY
    - Supported platforms:
        - Linux [Netfilter TPROXY](https://docs.kernel.org/networking/tproxy.html)
        - FreeBSD [ipfw forward](https://man.freebsd.org/cgi/man.cgi?query=ipfw)
        - OpenBSD [pf divert-to](https://man.openbsd.org/pf.conf.5#divert-to)
    - Fact-based user authentication

#### Reverse Proxy Servers

- HTTP(S) Reverse Proxy
    - TLS / mTLS
    - Basic user authentication
    - Port hiding
    - Host-based routing

#### Streaming Servers

- TCP Stream
    - Upstream TLS / mTLS
    - Load balancing: RR / Random / Rendezvous / Jump Hash
    - Fact-based user authentication

- TLS Stream
    - mTLS
    - Upstream TLS / mTLS
    - Load balancing: RR / Random / Rendezvous / Jump Hash
    - Fact-based user authentication

#### Port Alias Servers

Port alias servers add additional listening ports in front of other servers.

- Plain TCP Port
    - PROXY Protocol

- Plain TLS Port
    - PROXY Protocol
    - mTLS
    - Based on Rustls

- Native TLS Port
    - PROXY Protocol
    - mTLS
    - Based on OpenSSL / BoringSSL / AWS-LC / Tongsuo

- Intelli Proxy Port
    - Multiple protocols: HTTP Proxy and SOCKS Proxy
    - PROXY Protocol

### Escapers

Escapers define how vey-proxy connects to upstream targets. Multiple escaper types are available for different
outbound strategies.

Common capabilities include:

- Happy Eyeballs
- Socket speed limits
- Extensive TCP and UDP socket configuration
- Source IP binding

#### Direct Connect Escapers

- DirectFixed
    - TCP CONNECT / TLS CONNECT / HTTP(S) forward / UDP ASSOCIATE
    - Egress network filtering
    - DNS rewrite support
    - Index-based egress path selection

- DirectFloat
    - TCP CONNECT / TLS CONNECT / HTTP(S) forward / UDP ASSOCIATE
    - Egress network filtering
    - DNS rewrite support
    - Dynamic source IP binding
    - JSON-based egress path selection

#### Proxy Chaining Escapers

- HTTP Proxy
    - TCP CONNECT / TLS CONNECT / HTTP(S) forward
    - PROXY Protocol
    - Load balancing: RR / Random / Rendezvous / Jump Hash
    - Basic user authentication

- HTTPS Proxy
    - TCP CONNECT / TLS CONNECT / HTTP(S) forward
    - PROXY Protocol
    - Load balancing: RR / Random / Rendezvous / Jump Hash
    - Basic user authentication
    - mTLS

- SOCKS5(S) Proxy
    - TCP CONNECT / TLS CONNECT / HTTP(S) forward / UDP ASSOCIATE
    - Load balancing: RR / Random / Rendezvous / Jump Hash
    - Basic user authentication

- ProxyFloat
    - Dynamic proxy selection across HTTP Proxy / HTTPS Proxy / SOCKS5(S) Proxy
    - JSON-based egress path selection

#### Routing Escapers

Routing escapers choose the actual upstream escaper based on routing rules.

- `route-client`: route by client address
    - Exact IP match
    - Subnet match

- `route-mapping`: route by user-provided request rules
    - Index-based egress path selection

- `route-query`: route using an external agent

- `route-resolved`: route by the resolved IP of the target host

- `route-geoip`: route by GeoIP rules for the resolved IP

- `route-select`: simple load balancer
    - RR / Random / Rendezvous / Jump Hash
    - JSON-based egress path selection

- `route-upstream`: route by the original target host
    - Exact IP match
    - Exact domain match
    - Wildcard domain match
    - Subnet match
    - Regex domain match

- `route-failover`: failover between primary and standby escapers

#### Helper Escapers

- `comply-audit`: override server-side auditor settings
- `comply-context`: update egress path config based on egress context

### Resolvers

- `c-ares`
    - UDP
    - TCP

- `hickory`
    - UDP / TCP
    - DNS over TLS
    - DNS over HTTPS
    - DNS over HTTP/3
    - DNS over QUIC

- `fail-over`

### Authentication

#### Authentication Methods

- Fact-based
- Basic username/password
    - LDAP simple bind
    - HTTP Basic authentication
    - SOCKS5 user authentication
- Anonymous user

#### User Sources

- Dynamic fetch
    - Local file
    - Lua script
    - Python script
- LDAP auto-discovery

#### User Features

- ACLs for proxy requests, target hosts, target ports, and user agents
- Socket speed limits and process-wide global speed limits
- Request rate limits, concurrency limits, and idle detection
- Automatic expiration and blocking
- JSON-based egress path selection

#### User Site Features

You can also define site-specific settings for each user:

- Match by exact IP, exact domain, wildcard domain, or subnet
- Request metrics, client traffic metrics, and remote traffic metrics
- Task duration histogram metrics
- Custom TLS client configuration

### Auditing

- TCP protocol inspection
- Task-level sampling
- TLS/TLCP interception
- External certificate generator integration
- TLS/TLCP decrypted stream export
- Stream detours for connection-oriented protocols
- HTTP/1 and HTTP/2 interception
- IMAP and SMTP interception
- ICAP adaptation for HTTP/1 / HTTP/2 / IMAP / SMTP

### Logging

- Log types
    - Server: task logs
    - Escaper: upstream connection error logs
    - Resolver: resolution error logs
    - Auditor: inspection and interception logs

- Backends
    - journald
    - syslog
    - fluentd

### Metrics

- Metric types
    - Server-level metrics
    - Escaper-level metrics
    - User-level metrics
    - User-site metrics
    - Resolver metrics
    - Runtime metrics
    - Log metrics

- Backends
    - StatsD, which can then fan out into many other TSDBs through existing StatsD-compatible pipelines
