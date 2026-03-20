[![minimum rustc: 1.90](https://img.shields.io/badge/minimum%20rustc-1.90-green?logo=rust)](https://www.whatrustisit.com)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](LICENSE)
[![codecov](https://codecov.io/gh/VEY-OSS/vey/graph/badge.svg?token=c8gK1HrRsX)](https://codecov.io/gh/VEY-OSS/vey)
[![docs](https://readthedocs.org/projects/vey/badge)](https://vey.readthedocs.io/)

# VEY - Versatile Edge WAY

[中文版 README](README.zh_CN.md) | [日本語 README](README.ja_JP.md)

## About

The VEY project is designed for building enterprise-grade general-purpose proxy solutions, including but not limited to
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

A general-purpose forward proxy solution that also includes basic support for TCP streaming, transparent proxying, and
reverse proxying.

#### Feature highlights

- Async Rust for speed and reliability
- HTTP/1 and SOCKS5 forward proxy protocols, plus SNI proxy and TCP TPROXY
- Support for easy-proxy and the `masque/http` Well-Known URI
- Proxy chaining, including dynamic upstream proxy selection
- Multiple egress route selection methods, with support for custom egress selection agents
- TCP/TLS stream proxying and basic HTTP reverse proxy support
- TLS via OpenSSL / BoringSSL / AWS-LC / AWS-LC-FIPS / Tongsuo / rustls
- TLS MITM interception, decrypted traffic export, and HTTP/1 / HTTP/2 / IMAP / SMTP interception
- ICAP integration for HTTP/1 / HTTP/2 / IMAP / SMTP, with straightforward integration into third-party security products
- Graceful reload
- Customizable load balancing and failover strategies
- User authentication with extensive configuration options
- Per-user site-specific configuration
- Rich ACL and limit rules at the ingress, egress, and user levels
- Rich monitoring metrics at the ingress, egress, user, and user-site levels
- Support for a wide range of observability tools

[README](vey-proxy/README.md) | [User Guide](vey-proxy/UserGuide.en_US.md) |
[Reference Doc](https://vey.readthedocs.io/projects/proxy/en/latest/)

### vey-statsd

A StatsD-compatible metrics aggregator.

[README](vey-statsd/README.md) | [Reference Doc](https://vey.readthedocs.io/projects/statsd/en/latest/)

### vey-gateway

A reverse proxy solution currently under active development.

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

A simple implementation of a Cloudflare keyless server.

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
