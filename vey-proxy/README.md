[![docs](https://readthedocs.org/projects/vey-proxy/badge)](https://vey.readthedocs.io/projects/proxy/)

# VEY Proxy

`vey-proxy` is a programmable general-purpose proxy server. It supports controlled outbound access,
protocol-aware traffic handling, transparent proxy deployments, stream proxying, selective reverse proxying,
and proxy chaining. It combines multiple ingress server types, flexible egress routing, pluggable
authentication, DNS control, auditing, structured logging, and metrics export in one service.

It can be used as:

- a forward proxy for HTTP(S), SOCKS, and mixed client environments
- a transparent proxy for policy enforcement and selective interception
- a stream proxy for TCP and TLS services
- a basic reverse proxy for HTTP services
- an egress gateway that chooses upstream routes dynamically

The project is designed around composable modules. Servers accept client traffic, escapers decide how outbound
connections are made, resolvers control DNS behavior, auth modules define identity and policy, and auditors add
inspection or interception where needed.

## Architecture at a Glance

`vey-proxy` is built from a few core configuration object types:

- `server`
  Accepts inbound traffic. Different server types cover HTTP proxying, SOCKS proxying, reverse proxying,
  TCP/TLS stream proxying, transparent proxying, and additional listening-port wrappers.

- `escaper`
  Controls how upstream traffic leaves the service. Escapers can connect directly, chain through another proxy,
  or route traffic to another escaper according to client, upstream, GeoIP, or external policy.

- `resolver`
  Handles DNS resolution for escapers and routing logic. Both classic DNS and encrypted DNS transports are supported.

- `auth`
  Provides user identity, grouping, permissions, quotas, and policy decisions for authenticated traffic.

- `auditor`
  Adds protocol inspection, interception, traffic export, and adaptation workflows.

This separation makes it practical to combine a small set of reusable components into very different deployment
patterns without rewriting the whole configuration.

## User Guide

[中文](UserGuide.zh_CN.md) | [English](UserGuide.en_US.md)

The user guide focuses on installation, operational concepts, and common deployment patterns. It is the best place
to start if you want working examples before reading the full reference.

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

The main binaries are:

- `vey-proxy`: the proxy daemon
- `vey-proxy-ctl`: the local control and management CLI

## Documentation

The Sphinx-generated reference documentation is available on
[Read the Docs](https://vey.readthedocs.io/projects/proxy/en/latest/). It covers configuration formats, log formats,
metrics, protocol definitions, and related reference material.

Documentation entry points:

- [Configuration Reference](../sphinx/vey-proxy/configuration/index.rst)
- [Protocol Details](../sphinx/vey-proxy/protocol/index.rst)
- [Metrics Definition](../sphinx/vey-proxy/metrics/index.rst)
- [Log Format](../sphinx/vey-proxy/log/index.rst)

## Examples

Example configurations are available in [examples](examples). These examples are useful when you want to see how the
module model fits together in real YAML rather than reading option-by-option reference pages.

## Typical Use Cases

- Provide managed HTTP and SOCKS proxy access for users or applications.
- Route different destinations through different outbound links or upstream proxy providers.
- Build transparent proxy deployments based on TPROXY, `pf divert-to`, or `ipfw forward`.
- Enforce user-level policy with ACLs, bandwidth limits, concurrency controls, and site-specific overrides.
- Inspect, adapt, or export selected traffic for compliance and troubleshooting workflows.
- Expose stream-based internal services with TCP or TLS proxy frontends.

## Operational Highlights

- Modular configuration with independently reusable servers, escapers, resolvers, auth groups, and auditors
- Hot-reload-oriented deployment model with systemd-friendly service management
- Fine-grained routing based on client address, target host, resolved IP, GeoIP attributes, or external route queries
- Support for direct egress, static proxy chaining, and dynamic proxy discovery
- User and site policy controls for ACLs, quotas, rate limits, speed limits, and expiration
- Structured logs and StatsD-compatible metrics for downstream observability pipelines
- Multiple TLS stacks and optional TLCP support for deployments that need them

## Getting Started

If you are evaluating or deploying `vey-proxy`, this is the shortest practical path:

1. Set up the build or package environment with [dev-setup](../doc/dev-setup.md).
2. Read the [English user guide](UserGuide.en_US.md) for service structure and baseline concepts.
3. Start from one of the configs under [examples](examples).
4. Use the Sphinx reference when you need exact key names, supported value types, metrics, or log fields.

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
