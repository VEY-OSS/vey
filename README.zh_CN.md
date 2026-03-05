[![minimum rustc: 1.90](https://img.shields.io/badge/minimum%20rustc-1.90-green?logo=rust)](https://www.whatrustisit.com)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](LICENSE)
[![codecov](https://codecov.io/gh/VEY-OSS/vey/graph/badge.svg?token=c8gK1HrRsX)](https://codecov.io/gh/VEY-OSS/vey)
[![docs](https://readthedocs.org/projects/vey/badge)](https://vey.readthedocs.io/)

# VEY Project

[English README](README.md) | [日本語 README](README.ja_JP.md)

## 关于

VEY（Versatile Edge Way的缩写）项目可用于构建面向企业的通用代理解决方案， 包括但不限于代理、反向代理（开发中）、负载均衡（待定）、NAT穿透（开发中）等。

本项目由原作者fork自原[G3项目](https://github.com/bytedance/g3)。

## 应用程序

VEY项目包含许多应用，每一个应用程序单独一个子目录，包含各自的代码、文档等。

除了应用程序目录，还有一些公共目录：

- [doc](doc) 包含项目级别文档。
- [sphinx](sphinx) 用于为各应用生成HTML参考文档。
- [scripts](scripts) 包含各种辅助脚本，包括覆盖率测试、打包脚本等。

### g3proxy

通用正向代理解决方案，同时带有TCP映射、TLS卸载/封装、透明代理、简单反向代理等功能。

#### 主要特性

- Async Rust: 高效、稳定
- Http1 / Socks5 正向代理协议, SNI Proxy and TCP TPROXY
- 支持 easy-proxy & masque/http Well-Known URI
- 代理串联，动态下一级代理节点选择
- 丰富的出口路由选择方法，支持接入自定义选路Agent
- TCP/TLS反向代理，基础HTTP反向代理
- TLS支持OpenSSL / BoringSSL / AWS-LC / AWS-LC-FIPS / Tongsuo, 部分场景支持rustls
- TLS中间人劫持, 解密流量导出, HTTP1/HTTP2/IMAP/SMTP协议解析
- ICAP审计，支持HTTP1/HTTP2/IMAP/SMTP，无缝集成第三方安全审计/杀毒产品
- 优雅重载 & 热升级
- 灵活的负载均衡&容灾策略
- 用户认证，且支持丰富的配置选项
- 用户配置下可细化进行差异化站点配置
- 丰富的ACL/限流限速策略，包括入口/出口/用户维度
- 丰富的监控指标，包括入口/出口/用户/用户站点维度
- 多种日志 & 监控解决方案集成能力

[详细介绍](g3proxy/README.md) | [用户指南](g3proxy/UserGuide.zh_CN.md) |
[参考文档](https://g3-project.readthedocs.io/projects/g3proxy/en/latest/)

### g3statsd

StatsD兼容的监控打点指标聚合服务。

[详细介绍](g3statsd/README.md) | [参考文档](https://g3-project.readthedocs.io/projects/g3statsd/en/latest/)

### g3tiles

通用反向代理解决方案，开发中。

[参考文档](https://g3-project.readthedocs.io/projects/g3tiles/en/latest/)

### vey-bench

压测工具，支持

- HTTP: HTTP/1.1, HTTP/2, HTTP/3
- WebSocket
- TLS Handshake
- DNS: UDP, TCP, DNS over TLS, DNS over HTTP, DNS over QUIC, DNS over HTTP/3
- Thrift RPC
- Cloudflare Keyless

[详细介绍](vey-bench/README.md)

### vey-mkcert

用来生成 根CA / 中间CA / TLS服务端 / TLS客户端 / 国密服务端 / 国密客户端 证书的工具。

[详细介绍](vey-mkcert/README.md)

### vey-dcgen

适用于g3proxy TLS劫持功能的伪造证书生成服务应用。

[详细介绍](vey-dcgen/README.md)

### vey-iploc

适用于g3proxy GeoIP功能的IP Location查找服务应用。

[详细介绍](vey-iploc/README.md)

### g3keymess

Cloudflare Keyless Server的简单实现。

[详细介绍](g3keymess/README.md) |
[参考文档](https://g3-project.readthedocs.io/projects/g3keymess/en/latest/)

## 支持平台

目前提供对Linux系统的完整支持。

以下系统也可以编译：

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

预编译包可以在 [cloudsmith](https://cloudsmith.io/~g3-oqh/repos/) 找到。

但是仍然建议自行编译打包，具体方法参考 [Build and Package](doc/build_and_package.md)。

### 长期支持版本

参考 [Long-Term Support](doc/long-term_support.md).

## 贡献指南

参考 [Contributing](CONTRIBUTING.md)。

## 贡献者公约

参考[Code of Conduct](CODE_OF_CONDUCT.md)。

## 安全

请[创建security advisory](https://github.com/VEY-OSS/vey/security/advisories/new)，不要直接提issue。

## 许可证

本项目使用许可证为 [Apache-2.0 License](LICENSE)。
