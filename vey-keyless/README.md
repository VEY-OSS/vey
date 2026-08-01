[![docs](https://readthedocs.org/projects/vey-keyless/badge)](https://vey.readthedocs.io/projects/keyless/)

# VEY Keyless

`vey-keyless` is a dedicated private-key service for TLS deployments. Its primary
key-operation protocol is [Cloudflare Keyless SSL](https://blog.cloudflare.com/keyless-ssl-the-nitty-gritty-technical-details/),
and listening sockets can be composed in front of that protocol through pluggable
server types.

It is intended for deployments where TLS private-key operations should be handled
by a dedicated service rather than by the edge process that terminates client
connections. This makes it easier to centralize key handling, integrate with
hardware acceleration, and keep private-key access under tighter control.

At a high level, `vey-keyless` provides:

- a network service that accepts connections from front-end TLS systems
- pluggable server types, so listening sockets can be chained in front of the
  key-operation protocol
- the Cloudflare Keyless protocol for RSA/ECDSA/Ed25519 private-key operations
- pluggable private-key stores
- backend execution modes for local OpenSSL or OpenSSL async jobs
- structured logging and StatsD-compatible metrics

## Architecture

The main configuration areas are:

- `server`
  Accepts incoming connections. A server either speaks a key-operation protocol
  (`cloudflare`, the default) or only owns a listening socket and forwards every
  accepted connection to a next server (`plain_tcp_port`, `plain_tls_port`).
  A `cloudflare` server may omit `listen` and serve only the connections handed
  over by port servers.

- `store`
  Defines where private keys are loaded from.

- `backend`
  Defines how private-key operations are executed.

- `log` and `stat`
  Control observability output.

## Building

Set up the build environment first by following [dev-setup](../doc/dev-setup.md).

Build debug binaries:

```shell
cargo build -p vey-keyless -p vey-keyless-ctl
```

Build release binaries:

```shell
cargo build --profile release-lto -p vey-keyless -p vey-keyless-ctl
```

If you want to build binary packages or container images, see
[Build and Package](../doc/build_and_package.md).

The main binaries are:

- `vey-keyless`: the keyless daemon
- `vey-keyless-ctl`: the local control and management CLI

## Documentation

The Sphinx-generated reference documentation is available on
[Read the Docs](https://vey.readthedocs.io/projects/keyless/en/latest/).

It covers:

- configuration
- log format
- metrics
- shared value types through the `vey-values` reference

## Features

`vey-keyless` uses the system OpenSSL by default.

You can choose different TLS/crypto libraries with feature flags:

- vendored-openssl

  Use the latest OpenSSL.

- vendored-boringssl

  Use BoringSSL.

- vendored-tongsuo

  Use Tongsuo.

The `plain_tls_port` server uses rustls, whose crypto provider is selected with
one of these mutually exclusive feature flags:

- rustls-ring (default)
- rustls-aws-lc
- rustls-aws-lc-fips

### Hardware Acceleration

It is possible to use hardware crypto engines through
[OpenSSL ENGINES](https://github.com/openssl/openssl/blob/master/README-ENGINES.md) or
[OpenSSL PROVIDERS](https://github.com/openssl/openssl/blob/master/README-PROVIDERS.md).

Enable OpenSSL async-job support with:

```text
cargo build --features openssl-async-job
```

You can build a hardware engine against the system OpenSSL, and enable it
in [openssl.cnf](https://docs.openssl.org/master/man5/config/). If you don't want
to change the default `openssl.cnf`, you can create a separate file and export it
through the `OPENSSL_CONF` environment variable.

See [Intel QAT Engine](IntelQatEngine.md) for a concrete setup example.

For a direct Intel `crypto_mb` multi-buffer backend (RSA-2048/3072/4096, ECDSA
P-256/P-384/P-521, Ed25519; **x86_64 only**), install `libcrypto_mb` and build
with:

```text
cargo build --features crypto-mb
```

Then set `backend: crypto_mb` (requires worker runtimes and a multiplex-enabled
`cloudflare` server). Single-request batches still use OpenSSL; batches of two or
more use `crypto_mb`.

## Typical Use Cases

- Run a dedicated keyless service behind an existing TLS edge.
- Keep private keys on a restricted host instead of on every front-end node.
- Terminate TLS with rustls on a `plain_tls_port`, then hand the stream to a
  `cloudflare` key-operation server.
- Combine keyless processing with OpenSSL engines or provider-based hardware
  acceleration.
- Separate network termination from cryptographic key use for operational control.

## Examples

Example configurations are available in [examples](examples):

- [simple_openssl](examples/simple_openssl): a single `cloudflare` server with its
  own TLS listen socket
- [tls_port](examples/tls_port): `plain_tls_port` and `plain_tcp_port` chained to a
  `cloudflare` server that has no listen socket of its own
- [worker_openssl](examples/worker_openssl): multi-worker setup with OpenSSL
