# vey-gateway

`vey-gateway` is a work-in-progress general-purpose reverse proxy and gateway
daemon in the Vey project. It is intended to grow into a broader programmable
gateway framework for multiple frontend and upstream protocols.

The current implementation focuses on TLS- and keyless-related traffic
handling. Even in its current state, the daemon already provides structured
logs, StatsD metrics, hot-reloadable runtime objects, and a modular
server/backend/discover model.

## Architecture

`vey-gateway` is built around a small set of core object types:

- `server` accepts client traffic and defines the frontend protocol behavior
- `discover` resolves or expands upstream targets
- `backend` connects to upstream services and handles request forwarding
- `log` configures structured event logging
- `stat` exports runtime and traffic metrics through StatsD

In a typical deployment, a server accepts traffic, creates a task, selects a
backend, asks the backend's discover object for candidate upstream peers, and
then proxies the request to the selected target.

## Current Server Roles

The current implementation and documentation cover these server families:

- `openssl_proxy` for OpenSSL-based TLS reverse proxying
- `rustls_proxy` for Rustls-based TLS reverse proxying
- `keyless_proxy` for keyless protocol proxying
- `plain_tcp_port` for TCP listener chaining
- `plain_quic_port` for QUIC listener chaining
- `dummy_close` for testing or explicit deny paths

## Current Backend Roles

The current implementation and documentation cover these backend families:

- `stream_tcp` for generic TCP upstream forwarding
- `keyless_tcp` for keyless over TCP or TLS
- `keyless_quic` for keyless over QUIC
- `dummy_close` for immediate close behavior

Discovery objects are documented separately and currently include static
address resolution and host-based resolution through the system resolver.

## Build

Build the release binary with:

```bash
cargo build --profile release-lto --bin vey-gateway
```

The binary will be generated under `target/release-lto/`.

## Documentation

The Sphinx documentation for `vey-gateway` lives under
[`sphinx/vey-gateway`](../sphinx/vey-gateway).

Build the HTML docs with:

```bash
cd sphinx/vey-gateway
make html
```

The generated output is written to `sphinx/vey-gateway/_build/html/`.

## Operations

`vey-gateway` is intended for long-running service deployments and already
includes:

- structured task logs for connection and keyless traffic
- StatsD metrics for servers, backends, runtimes, and loggers
- graceful shutdown controls for draining listeners and live tasks
- shared configuration value types documented in the `vey-values` Sphinx project

## Current Use Cases

- fronting TLS services with OpenSSL or Rustls while selecting upstream
  backends by SNI and ALPN
- forwarding keyless traffic to TCP or QUIC-based key servers
- exposing plain TCP or QUIC ports in front of internal gateway chains
- exporting operational metrics and structured logs to existing observability
  pipelines
