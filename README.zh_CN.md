[![minimum rustc: 1.90](https://img.shields.io/badge/minimum%20rustc-1.90-green?logo=rust)](https://www.whatrustisit.com)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](LICENSE)
[![codecov](https://codecov.io/gh/VEY-OSS/vey/graph/badge.svg?token=c8gK1HrRsX)](https://codecov.io/gh/VEY-OSS/vey)
[![docs](https://readthedocs.org/projects/vey/badge)](https://vey.readthedocs.io/)

# VEY - Versatile Edge WAY

[English README](README.md) | [日本語 README](README.ja_JP.md)

## 关于

VEY（Versatile Edge Way）是一个面向企业场景的通用代理解决方案项目，可用于构建正向代理、反向代理（开发中）、
负载均衡（待定）、NAT 穿透（开发中）等能力。

本项目由 [G3 项目](https://github.com/bytedance/g3) 原作者 fork 并发起。

## 应用程序

VEY 项目由多个应用组成，每个应用都有独立的子目录，用于存放各自的代码、文档及相关资源。

除各应用目录外，仓库中还包含一些公共目录：

- [doc](doc) 包含项目级文档。
- [sphinx](sphinx) 用于生成各应用的 HTML 参考文档。
- [scripts](scripts) 包含各类辅助脚本，例如覆盖率测试和打包脚本。

### vey-proxy

通用正向代理解决方案，同时提供 TCP 映射、TLS 卸载/封装、透明代理、基础反向代理等能力。

#### 主要特性

- Async Rust：高性能、高可靠
- 支持 HTTP/1、SOCKS5 正向代理协议，以及 SNI Proxy 和 TCP TPROXY
- 支持 easy-proxy 与 `masque/http` Well-Known URI
- 代理串联，动态下一级代理节点选择
- 丰富的出口路由选择方式，支持接入自定义选路 Agent
- TCP/TLS 流代理与基础 HTTP 反向代理
- TLS 支持 OpenSSL / BoringSSL / AWS-LC / AWS-LC-FIPS / Tongsuo，部分场景支持 rustls
- TLS 中间人劫持、解密流量导出，以及 HTTP/1 / HTTP/2 / IMAP / SMTP 协议解析
- 支持 HTTP/1 / HTTP/2 / IMAP / SMTP 的 ICAP 适配，可无缝集成第三方安全审计/杀毒产品
- 优雅重载与热升级
- 灵活的负载均衡与容灾策略
- 用户认证，且支持丰富的配置选项
- 支持按用户细化站点级差异化配置
- 丰富的 ACL / 限流 / 限速策略，覆盖入口 / 出口 / 用户等维度
- 丰富的监控指标，覆盖入口 / 出口 / 用户 / 用户站点等维度
- 支持多种日志与监控方案集成

[详细介绍](vey-proxy/README.md) | [用户指南](vey-proxy/UserGuide.zh_CN.md) |
[参考文档](https://vey.readthedocs.io/projects/proxy/en/latest/)

### vey-statsd

兼容 StatsD 的指标聚合服务。

[详细介绍](vey-statsd/README.md) | [参考文档](https://vey.readthedocs.io/projects/statsd/en/latest/)

### vey-gateway

通用反向代理解决方案，当前仍在开发中。

[参考文档](https://vey.readthedocs.io/projects/gateway/en/latest/)

### vey-bench

压测工具，支持：

- HTTP: HTTP/1.1, HTTP/2, HTTP/3
- WebSocket
- TLS Handshake
- DNS: UDP, TCP, DNS over TLS, DNS over HTTP, DNS over QUIC, DNS over HTTP/3
- Thrift RPC
- Cloudflare Keyless

[详细介绍](vey-bench/README.md)

### vey-mkcert

用于生成根 CA / 中间 CA / TLS 服务端 / TLS 客户端 / 国密服务端 / 国密客户端证书的工具。

[详细介绍](vey-mkcert/README.md)

### vey-dcgen

适用于 vey-proxy TLS 劫持功能的动态证书生成服务。

[详细介绍](vey-dcgen/README.md)

### vey-iploc

适用于 vey-proxy GeoIP 功能的 IP 地理位置查询服务。

[详细介绍](vey-iploc/README.md)

### vey-keyless

Cloudflare Keyless Server 的简易实现。

[详细介绍](vey-keyless/README.md) |
[参考文档](https://vey.readthedocs.io/projects/keyless/en/latest/)

## 支持平台

目前已完整支持 Linux。

以下平台也可以编译：

- macOS
- Windows >= 10
- FreeBSD >= 14.3
- NetBSD >= 10.1
- OpenBSD >= 7.8

## 开发环境搭建

参考 [Dev-Setup](doc/dev-setup.md)。

## 标准及约定

参考 [Standards](doc/standards.md)。

## 构建、打包及部署

预编译包可在 [cloudsmith](https://cloudsmith.io/~vey-oss/repos/) 获取。

但仍推荐自行编译并打包，具体方法请参考 [Build and Package](doc/build_and_package.md)。

### 长期支持版本

参考 [Long-Term Support](doc/long-term_support.md)。

## 贡献指南

参考 [Contributing](CONTRIBUTING.md)。

## 贡献者公约

参考 [Code of Conduct](CODE_OF_CONDUCT.md)。

## 安全

如发现安全问题，请在 GitHub 上
[创建 security advisory 草稿](https://github.com/VEY-OSS/vey/security/advisories/new)，不要直接提交公开 issue。

## 许可证

本项目基于 [Apache-2.0 License](LICENSE) 发布。
