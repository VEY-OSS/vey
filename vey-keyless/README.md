[![docs](https://readthedocs.org/projects/vey-keyless/badge)](https://vey.readthedocs.io/projects/keyless/)

# VEY Keyless

`vey-keyless` is a server implementation of the Cloudflare Keyless SSL
protocol.

It is intended for deployments where TLS private-key operations should be
handled by a dedicated service rather than by the edge process that terminates
client connections. This makes it easier to centralize key handling, integrate
with hardware acceleration, and keep private-key access under tighter control.

At a high level, `vey-keyless` provides:

- a network service that accepts keyless requests from front-end TLS systems
- pluggable private-key stores
- backend execution modes for local OpenSSL or OpenSSL async jobs
- structured logging and StatsD-compatible metrics

## Architecture

The main configuration areas are:

- `server`
  Accepts incoming keyless protocol requests.

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
to change the default `openssl.cnf`, you can create a separate file and export
it through the `OPENSSL_CONF` environment variable.

See [Intel QAT Engine](IntelQatEngine.md) for a concrete setup example.

## Typical Use Cases

- Run a dedicated keyless service behind an existing TLS edge.
- Keep private keys on a restricted host instead of on every front-end node.
- Combine keyless processing with OpenSSL engines or provider-based hardware acceleration.
- Separate network termination from cryptographic key use for operational control.

## Examples

Example configurations are available in [examples](examples).
