[![minimum rustc: 1.90](https://img.shields.io/badge/minimum%20rustc-1.90-green?logo=rust)](https://www.whatrustisit.com)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](LICENSE)
[![codecov](https://codecov.io/gh/VEY-OSS/vey/graph/badge.svg?token=c8gK1HrRsX)](https://codecov.io/gh/VEY-OSS/vey)
[![docs](https://readthedocs.org/projects/vey/badge)](https://vey.readthedocs.io/)

# VEY - Versatile Edge WAY

[中文版 README](README.zh_CN.md) | [English README](README.md)

## 概要

VEY は、エンタープライズ向けの汎用プロキシソリューションを構築するためのプロジェクトです。フォワードプロキシ /
リバースプロキシ (WIP) / ロードバランサー (TBD) / NAT トラバーサル (WIP) など、幅広い用途に利用できます。

このプロジェクトは、[G3 プロジェクト](https://github.com/bytedance/g3) の作者によるフォークです。

## アプリケーション

VEY プロジェクトは複数のアプリケーションで構成されており、それぞれにコード、ドキュメント、関連ファイルを含む
専用のサブディレクトリがあります。

各アプリケーションのディレクトリに加えて、リポジトリには共通で利用するディレクトリもあります。

- [doc](doc): プロジェクト全体のドキュメント。
- [sphinx](sphinx): 各アプリケーションの HTML リファレンスドキュメントを生成するためのソース一式。
- [scripts](scripts): カバレッジテスト、パッケージングスクリプトなどの各種補助スクリプト。

### vey-proxy

汎用フォワードプロキシソリューションです。基本機能として TCP ストリーミング / トランスペアレントプロキシ /
リバースプロキシにも対応しています。

#### 主な機能

- 非同期 Rust による高い性能と信頼性
- HTTP/1 および SOCKS5 のフォワードプロキシ、SNI Proxy、TCP TPROXY
- easy-proxy と masque/http Well-Known URI をサポート
- プロキシチェーンに対応し、上流プロキシの動的選択も可能
- 多彩な出口ルート選択方式に対応し、カスタム出口選択エージェントも利用可能
- TCP/TLS ストリームプロキシと基本的な HTTP リバースプロキシ機能
- OpenSSL / BoringSSL / AWS-LC / AWS-LC-FIPS / Tongsuo / rustls による TLS
- TLS MITM インターセプト、復号済みトラフィックのダンプ、HTTP/1・HTTP/2・IMAP・SMTP のインターセプト
- HTTP/1・HTTP/2・IMAP・SMTP 向けの ICAP 連携に対応し、サードパーティ製セキュリティ製品とも容易に統合可能
- グレースフルリロード
- カスタマイズ可能なロードバランシングおよびフェイルオーバー戦略
- ユーザー認証と豊富な設定オプション
- ユーザーごとに異なるサイト設定を適用可能
- 入口 / 出口 / ユーザーレベルで豊富な ACL / 制限ルールを提供
- 入口 / 出口 / ユーザー / ユーザーサイト単位で豊富な監視メトリクスを提供
- さまざまなオブザーバビリティツールをサポート

[README](vey-proxy/README.md) | [ユーザーガイド](vey-proxy/UserGuide.en_US.md) |
[リファレンスドキュメント](https://vey.readthedocs.io/projects/proxy/en/latest/)

### vey-statsd

StatsD 互換のメトリクス集約ツールです。

[README](vey-statsd/README.md) | [リファレンスドキュメント](https://vey.readthedocs.io/projects/statsd/en/latest/)

### vey-gateway

現在開発中のリバースプロキシソリューションです。

[リファレンスドキュメント](https://vey.readthedocs.io/projects/gateway/en/latest/)

### vey-bench

以下をサポートするベンチマークツールです。

- HTTP: HTTP/1.1, HTTP/2, HTTP/3
- WebSocket
- TLS Handshake
- DNS: UDP, TCP, DNS over TLS, DNS over HTTP, DNS over QUIC, DNS over HTTP/3
- Thrift RPC
- Cloudflare Keyless

[README](vey-bench/README.md)

### vey-mkcert

Root CA / Intermediate CA / TLS サーバー / TLS クライアント / TLCP サーバー / TLCP クライアント証明書を生成するためのツールです。

[README](vey-mkcert/README.md)

### vey-dcgen

vey-proxy 向けの動的証明書ジェネレーターです。

[README](vey-dcgen/README.md)

### vey-iploc

vey-proxy の GeoIP サポート向け IP ジオロケーション検索サービスです。

[README](vey-iploc/README.md)

### vey-keyless

Cloudflare keyless server のシンプルな実装です。

[README](vey-keyless/README.md) |
[リファレンスドキュメント](https://vey.readthedocs.io/projects/keyless/en/latest/)

## 対応プラットフォーム

Linux を完全にサポートしています。

以下のプラットフォームでもコンパイルできます。

- macOS
- Windows >= 10
- FreeBSD >= 14.3
- NetBSD >= 10.1
- OpenBSD >= 7.8

## 開発環境セットアップ

[Dev-Setup](doc/dev-setup.md) を参照してください。

## 標準

[Standards](doc/standards.md) を参照してください。

## ビルド、パッケージング、デプロイ

ビルド済みパッケージは [cloudsmith](https://cloudsmith.io/~vey-oss/repos/) から入手できます。

ただし、可能であればパッケージは自前でビルドすることを推奨します。詳細は
[Build and Package](doc/build_and_package.md) を参照してください。

### LTS バージョン

[Long-Term Support](doc/long-term_support.md) を参照してください。

## コントリビューション

詳細は [Contributing](CONTRIBUTING.md) を参照してください。

## 行動規範

詳細は [Code of Conduct](CODE_OF_CONDUCT.md) を参照してください。

## セキュリティ

セキュリティ上の問題は、GitHub の
[draft security advisory](https://github.com/VEY-OSS/vey/security/advisories/new) を作成して報告してください。

公開の GitHub Issue は作成しないでください。

## ライセンス

このプロジェクトは [Apache-2.0 License](LICENSE) のもとで提供されています。
