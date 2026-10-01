[![docs](https://readthedocs.org/projects/vey-proxy/badge)](https://vey.readthedocs.io/projects/proxy/)

# VEY Proxy

`vey-proxy` is a programmable general-purpose proxy server. It supports controlled outbound access,
protocol-aware traffic handling, transparent proxy deployments, stream proxying, HTTP and TLS reverse
proxying, and proxy chaining. It combines multiple ingress server types, flexible egress routing, pluggable
authentication, DNS control, auditing, structured logging, and metrics export in one service.

It can be used as:

- a forward proxy for HTTP(S), SOCKS, and mixed client environments
- a transparent proxy for policy enforcement and selective interception
- a stream proxy for TCP and TLS services
- a reverse proxy for HTTP services and TLS streams, with a per-site origin
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

- `site group`
  Holds the Host and SNI table used by reverse-proxy servers. Each site stores the origin, certificates,
  limits, and HTTP origin settings. This is separate from the per-user destination overrides on a
  forward-proxy user.

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
- Publish HTTP or TLS services through a reverse-proxy frontend, with per-site certificates, origin selection,
  and tenant limits.

## Operational Highlights

- Modular configuration with independently reusable servers, escapers, resolvers, auth groups, and auditors
- Hot-reload-oriented deployment model with systemd-friendly service management
- Fine-grained routing based on client address, target host, resolved IP, GeoIP attributes, or external route queries
- Support for direct egress, static proxy chaining, and dynamic proxy discovery
- User, user-site, and reverse-proxy site policy controls for ACLs, quotas, rate limits, speed limits, and expiration
- Structured logs and StatsD-compatible metrics, including reverse-proxy site metrics, for downstream observability pipelines
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
    - HTTP forward, HTTPS forward, HTTP CONNECT, FTP over HTTP and HTTP CONNECT-UDP
    - `easy-proxy` Well-Known URI support
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

- UDP TPROXY
    - Supported platforms:
        - Linux [Netfilter TPROXY](https://docs.kernel.org/networking/tproxy.html)
        - FreeBSD [ipfw forward](https://man.freebsd.org/cgi/man.cgi?query=ipfw)
        - OpenBSD [pf divert-to](https://man.openbsd.org/pf.conf.5#divert-to)
    - Fact-based user authentication

#### Reverse Proxy Servers

Reverse-proxy servers pick an origin from a `site_group` by Host or SNI. A site holds one upstream address
or a weighted list of IP addresses, ingress and egress TLS, request limits, and HTTP origin settings.
`vey-proxy-ctl reload-site-group` reloads one group. Site stats and limiters stay when the site ID is unchanged.
Runtime upstream weights can be read and changed with `site-upstream` and `set-site-upstream-weight`.

Shared site capabilities:

- Exact host match and suffix match. `http_expose` can also use a group default site
- Import sites from another group by tag
- Upstream selection: round-robin, serial, rendezvous, ketama, or jump hash
- Origin TLS / mTLS, including TLCP
- Per-site speed, request rate, concurrency, and idle limits
- Tenant identity from `site.owner`, with tenant limits and egress overrides constrained by the site
- `X-Forwarded-*` or `Forwarded`, kept from the previous hop only for listed client addresses
- HTTP/1 origin keepalive and a per-worker idle pool
- HTTP/2 origin multiplex pool
- Site metrics, separate from forward-proxy user-site metrics

Servers:

- HTTP Expose (`http_expose`)
    - Internal HTTP/1 reverse proxy
    - Optional visitor authentication (Basic)
    - Host selects the site; TLS SNI selects the certificate only
    - Unmatched hosts can fall through to the group default site
    - Optional suppression of early protocol-error replies
    - `http_rproxy` remains a deprecated alias
    - No auditor

- HTTP Guard (`http_guard`)
    - Public-edge HTTP reverse proxy
    - HTTP/1.0 and HTTP/1.1, including WebSocket upgrade
    - HTTP/2 over TLS when ALPN is `h2` and SNI matches a site, including RFC 8441 WebSocket
    - TLS and TLCP detected on the same listener; plaintext HTTP/2 is dropped
    - SNI is pinned for the HTTP/2 connection; a later Host for another site is rejected
    - Unmatched names are rejected locally and are not sent to a default origin
    - No visitor authentication; the tenant comes from `site.owner`
    - Optional ICAP (REQMOD / RESPMOD)
    - gRPC and HTTP/3 are not supported

- TLS Proxy (`tls_proxy`)
    - TLS termination selected by SNI, then a byte copy to the site upstream
    - Per-site certificate and upstream; sites without `tls_server` are skipped
    - Optional inspection of the inner stream, with ICAP when the auditor finds HTTP
    - No visitor authentication; the tenant comes from `site.owner`

#### Streaming Servers

- TCP Stream
    - Upstream TLS / mTLS
    - Load balancing: RR / Random / Rendezvous / Jump Hash
    - Fact-based user authentication

- UDP Stream
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
- Basic username/password, check by checked with
    - Local password records written in user config
    - LDAP simple bind to a remote LDAP server
    - A Python script that can be customized to support all other methods
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

These settings apply after a forward-proxy user is authenticated. They are not the sites inside a reverse-proxy
site group.

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
    - Reverse-proxy site metrics
    - Resolver metrics
    - Runtime metrics
    - Log metrics

- Backends
    - StatsD, which can then fan out into many other TSDBs through existing StatsD-compatible pipelines
