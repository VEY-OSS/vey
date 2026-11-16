[![minimum rustc: 1.100](https://img.shields.io/badge/minimum%20rustc-1.100-green?logo=rust)](https://www.whatrustisit.com)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](LICENSE)
[![codecov](https://codecov.io/gh/VEY-OSS/vey/graph/badge.svg?token=c8gK1HrRsX)](https://codecov.io/gh/VEY-OSS/vey)
[![docs](https://readthedocs.org/projects/vey/badge)](https://vey.readthedocs.io/)

![VEY banner](assets/banner.svg)

[中文版 README](README.zh_CN.md) | [日本語 README](README.ja_JP.md)

## About

VEY is a modular suite of networking daemons and CLI tools written in Rust. Its general-purpose applications cover
unified forward/reverse/transparent/stream proxying with built-in security auditing (`vey-proxy`), TLS and Keyless SSL
gateway/private-key services (`vey-gateway`, `vey-keyless`), StatsD metrics aggregation and forwarding (`vey-statsd`),
and multi-protocol benchmarking (`vey-bench`), complemented by companion services (`vey-dcgen`, `vey-iploc`) and a
certificate utility (`vey-mkcert`), with load-balancing (TBD) and NAT traversal (WIP) planned.

This project is a fork of [the G3 project](https://github.com/bytedance/g3) by its creator.

If you are migrating an existing G3 deployment, see
[Migration from G3 to VEY](doc/migrate_from_g3_to_vey.md).

## Applications

The VEY project consists of multiple applications, each with its own subdirectory for code, documentation, and related
assets.

In addition to the application directories, the repository also includes several shared directories:

- [doc](doc) contains project-level documentation.
- [sphinx](sphinx) contains the sources used to generate HTML reference documentation for each application.
- [scripts](scripts) contains helper scripts, including coverage and packaging utilities.

### vey-proxy

A modular proxy daemon written in async Rust. It unifies HTTP(S) and SOCKS4/5
forward proxying, HTTP and TLS reverse proxying, SNI and TPROXY transparent
proxying, and TCP/UDP/TLS stream proxying in a single hot-reloadable binary—with
built-in security auditing (TLS/TLCP interception, DPI, ICAP adaptation, stream
detours, and decrypted-traffic export), programmable egress routing, per-user and
per-site policies, and StatsD observability.

#### Feature highlights

- Async Rust implementation with graceful hot reload and multi-worker scaling
- HTTP(S) and SOCKS4/5 forward proxy support, including HTTP CONNECT-UDP and SOCKS5 UDP ASSOCIATE
- SNI transparent proxy and TCP/UDP transparent proxying via Linux TPROXY, FreeBSD `ipfw`, and OpenBSD `pf`
- HTTP and TLS reverse proxying via site groups: HTTP/1, HTTP/2 over TLS, WebSocket, passive peer health checks, and
  per-site origins and tenant limits
- TCP/UDP/TLS stream proxying with PROXY Protocol support
- Built-in security auditing: task-level sampling, TLS/TLCP interception (with dynamic cert generation via `vey-dcgen`),
  decrypted-traffic export, stream detours, and DPI for HTTP/1, HTTP/2, WebSocket, IMAP, and SMTP
- ICAP (`REQMOD` / `RESPMOD`) integration for content inspection, DLP, and application-layer adaptation
- Proxy chaining and dynamic egress routing by client IP, target host, resolved IP, GeoIP, or external route-query
  agents
- Multi-stack TLS based on OpenSSL, BoringSSL, AWS-LC, AWS-LC-FIPS, Tongsuo (with TLCP), or rustls
- Pluggable authentication (local, LDAP, Lua, Python), ACLs, quotas, rate/bandwidth limits, and per-user site policies
- Structured task, inspect, and intercept audit logs (`journald`, `syslog`, `fluentd`) plus StatsD metrics across
  ingress, egress, user, user-site, and reverse-proxy site dimensions

[README](vey-proxy/README.md) | [User Guide](vey-proxy/UserGuide.en_US.md) |
[Reference Doc](https://vey.readthedocs.io/projects/proxy/en/latest/)

### vey-gateway

A work-in-progress general-purpose reverse proxy and gateway daemon. It is
designed as a programmable gateway framework for multiple frontend and
upstream protocols. The current implementation supports TLS- and
keyless-related traffic handling.

[README](vey-gateway/README.md) |
[Reference Doc](https://vey.readthedocs.io/projects/gateway/en/latest/)

### vey-keyless

A server implementation of the Cloudflare Keyless SSL protocol. It allows TLS
edge services to delegate private-key operations to a dedicated backend
service, making it easier to centralize key handling and integrate with
OpenSSL-based hardware acceleration.

[README](vey-keyless/README.md) |
[Reference Doc](https://vey.readthedocs.io/projects/keyless/en/latest/)

### vey-statsd

A StatsD-compatible metrics ingestion, aggregation, and forwarding service. It
can receive metrics from application daemons, normalize or aggregate them
through a modular pipeline, and export the results to downstream systems such
as Graphite, OpenTSDB, Prometheus, InfluxDB, VictoriaMetrics, QuestDB, or
OpenGemini.

[README](vey-statsd/README.md) |
[Reference Doc](https://vey.readthedocs.io/projects/statsd/en/latest/)

### vey-bench

A multi-protocol benchmark tool that supports:

- HTTP: HTTP/1.1, HTTP/2, HTTP/3
- WebSocket
- TLS Handshake
- DNS: UDP, TCP, DNS over TLS, DNS over HTTP, DNS over QUIC, DNS over HTTP/3
- Thrift RPC
- Cloudflare Keyless

[README](vey-bench/README.md)

## Companion Services and Utilities

### vey-dcgen

A dynamic certificate generator companion service for `vey-proxy` TLS interception.

[README](vey-dcgen/README.md) |
[Reference Doc](https://vey.readthedocs.io/projects/dcgen/en/latest/)

### vey-iploc

An IP geolocation lookup companion service for `vey-proxy` GeoIP routing.

[README](vey-iploc/README.md) |
[Reference Doc](https://vey.readthedocs.io/projects/iploc/en/latest/)

### vey-mkcert

A CLI utility for generating root CA, intermediate CA, TLS server, TLS client,
TLCP server, and TLCP client certificates.

[README](vey-mkcert/README.md)

## Target Platforms

See [Target Platforms](doc/target_platforms.md).

## Development Environment Setup

See [Dev-Setup](doc/dev-setup.md).

## Standards

See [Standards](doc/standards.md).

## Build, Package and Deploy

Prebuilt packages are available on [cloudsmith](https://cloudsmith.io/~vey-oss/repos/).

Container images for linux/amd64 and linux/arm64 are published to GHCR. `<version>` is the application release version,
such as `1.15.1`. Debian is the default tag. `:<version>-debian` and `:<version>-alpine` select the base image.

| Application                               | Image                                  | GitHub Packages                                                        |
|-------------------------------------------|----------------------------------------|------------------------------------------------------------------------|
| [vey-proxy](vey-proxy/docker/README.md)   | `ghcr.io/vey-oss/vey-proxy:<version>`  | [vey-proxy](https://github.com/VEY-OSS/vey/pkgs/container/vey-proxy)   |
| [vey-statsd](vey-statsd/docker/README.md) | `ghcr.io/vey-oss/vey-statsd:<version>` | [vey-statsd](https://github.com/VEY-OSS/vey/pkgs/container/vey-statsd) |
| [vey-dcgen](vey-dcgen/docker/README.md)   | `ghcr.io/vey-oss/vey-dcgen:<version>`  | [vey-dcgen](https://github.com/VEY-OSS/vey/pkgs/container/vey-dcgen)   |
| [vey-iploc](vey-iploc/docker/README.md)   | `ghcr.io/vey-oss/vey-iploc:<version>`  | [vey-iploc](https://github.com/VEY-OSS/vey/pkgs/container/vey-iploc)   |

That said, building packages yourself is still recommended. See [Build and Package](doc/build_and_package.md) for
details.

### LTS Version

See [Long-Term Support](doc/long-term_support.md).

## Community

Join the Larksuite (飞书) user group via
this [link](https://applink.feishu.cn/client/chat/chatter/add_by_link?link_token=a7an15cf-4851-41ee-9741-b3a223ab9998)
or by scanning the QR code below:

<a href="https://applink.feishu.cn/client/chat/chatter/add_by_link?link_token=a7an15cf-4851-41ee-9741-b3a223ab9998"><img src="assets/lark_group.svg" alt="Lark Group" width="200"/></a>

## Contribution

See [Contributing](CONTRIBUTING.md) for details.

## Code of Conduct

See [Code of Conduct](CODE_OF_CONDUCT.md) for details.

## Security

Please report security issues by
[opening a draft security advisory](https://github.com/VEY-OSS/vey/security/advisories/new) on GitHub.

Please do **not** create a public GitHub issue.

## License

This project is licensed under the [Apache-2.0 License](LICENSE).
