[![minimum rustc: 1.90](https://img.shields.io/badge/minimum%20rustc-1.90-green?logo=rust)](https://www.whatrustisit.com)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](LICENSE)
[![codecov](https://codecov.io/gh/VEY-OSS/vey/graph/badge.svg?token=c8gK1HrRsX)](https://codecov.io/gh/VEY-OSS/vey)
[![docs](https://readthedocs.org/projects/vey/badge)](https://vey.readthedocs.io/)

# VEY: A versatile proxy and gateway platform

[中文版 README](README.zh_CN.md) | [日本語 README](README.ja_JP.md)

## About

The VEY project is designed for building enterprise-oriented general-purpose proxy solutions, including but not limited to
forward proxies, reverse proxies (WIP), load balancers (TBD), and NAT traversal services (WIP).

This project is a fork of [the G3 project](https://github.com/bytedance/g3) by its creator.

## Applications

The VEY project consists of multiple applications, each with its own subdirectory for code, documentation, and related
assets.

In addition to the application directories, the repository also includes several shared directories:

- [doc](doc) contains project-level documentation.
- [sphinx](sphinx) contains the sources used to generate HTML reference documentation for each application.
- [scripts](scripts) contains helper scripts, including coverage and packaging utilities.

### vey-proxy

A feature-rich general-purpose proxy daemon. It centers on forward-proxy
workloads, while also supporting transparent proxying, TCP and TLS stream
proxying, selective reverse-proxy features, traffic inspection, and
policy-driven request handling.

#### Feature highlights

- High-performance async Rust implementation
- HTTP/1 and SOCKS5 forward proxy support, plus SNI proxy and TCP TPROXY
- Proxy chaining and multiple egress-route selection methods, including custom selection agents
- TCP/TLS stream proxying and basic HTTP reverse-proxy support
- TLS based on OpenSSL, BoringSSL, AWS-LC, AWS-LC-FIPS, Tongsuo, or rustls
- TLS interception, decrypted-traffic export, and HTTP/1, HTTP/2, IMAP, and SMTP inspection
- ICAP integration for common application-layer inspection workflows
- Rich authentication, ACL, rate-limit, and per-user policy controls
- Detailed metrics and logging for ingress, egress, user, and user-site dimensions
- Graceful reload plus flexible load-balancing and failover behavior

[README](vey-proxy/README.md) | [User Guide](vey-proxy/UserGuide.en_US.md) |
[Reference Doc](https://vey.readthedocs.io/projects/proxy/en/latest/)

### vey-statsd

A StatsD-compatible metrics ingestion, aggregation, and forwarding service. It
can receive metrics from application daemons, normalize or aggregate them
through a modular pipeline, and export the results to downstream systems such
as Graphite, OpenTSDB, or InfluxDB.

[README](vey-statsd/README.md) | [Reference Doc](https://vey.readthedocs.io/projects/statsd/en/latest/)

### vey-gateway

A work-in-progress general-purpose reverse proxy and gateway daemon. It is
designed as a programmable gateway framework for multiple frontend and
upstream protocols. The current implementation supports TLS- and
keyless-related traffic handling.

[README](vey-gateway/README.md) | 
[Reference Doc](https://vey.readthedocs.io/projects/gateway/en/latest/)

### vey-bench

A benchmark tool that supports:

- HTTP: HTTP/1.1, HTTP/2, HTTP/3
- WebSocket
- TLS Handshake
- DNS: UDP, TCP, DNS over TLS, DNS over HTTP, DNS over QUIC, DNS over HTTP/3
- Thrift RPC
- Cloudflare Keyless

[README](vey-bench/README.md)

### vey-mkcert

A tool for generating root CA, intermediate CA, TLS server, TLS client, TLCP server, and TLCP client certificates.

[README](vey-mkcert/README.md)

### vey-dcgen

A dynamic certificate generator for vey-proxy.

[README](vey-dcgen/README.md)

### vey-iploc

An IP geolocation lookup service for vey-proxy GeoIP support.

[README](vey-iploc/README.md)

### vey-keyless

A server implementation of the Cloudflare Keyless SSL protocol. It allows TLS
edge services to delegate private-key operations to a dedicated backend
service, making it easier to centralize key handling and integrate with
OpenSSL-based hardware acceleration.

[README](vey-keyless/README.md) |
[Reference Doc](https://vey.readthedocs.io/projects/keyless/en/latest/)

## Target Platforms

Linux is fully supported.

The code also compiles on the following platforms:

- macOS
- Windows >= 10
- FreeBSD >= 14.3
- NetBSD >= 10.1
- OpenBSD >= 7.8

## Development Environment Setup

See [Dev-Setup](doc/dev-setup.md).

## Standards

See [Standards](doc/standards.md).

## Build, Package and Deploy

Prebuilt packages are available on [cloudsmith](https://cloudsmith.io/~vey-oss/repos/).

That said, building packages yourself is still recommended. See [Build and Package](doc/build_and_package.md) for
details.

### LTS Version

See [Long-Term Support](doc/long-term_support.md).

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
