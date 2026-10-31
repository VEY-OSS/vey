# VEY DCGen

vey-dcgen is a dynamic server certificate generator/issuer to be used with vey-proxy to enable TLS interception.
The protocol is
defined [here](https://vey.readthedocs.io/projects/proxy/en/latest/protocol/helper/cert_generator.html).

It is designed to run with vey-proxy on the same host. You can also write you own implementation if you need to:

- serve a cluster of vey-proxy instances
- cache the generated server certificate for a much longer time
- add custom certificate issue methods

## How to build

To build debug binaries:

```shell
cargo build -p vey-dcgen
```

To build release binaries:

```shell
cargo build --profile release-lto -p vey-dcgen
```

To support SM2 certificates, you need to use *Tongsuo* by adding `--features vendored-tongsuo`.

See [Build and Package](../doc/build_and_package.md) if you want to build binary packages or docker images.
How to run the published images is described in [docker/README.md](docker/README.md).

## How to run

### Example

See this [simple example](examples/simple).

You can run `cargo run --bin vey-dcgen -- -c vey-dcgen/examples/simple/ -G example -vv` to start it.
The example listens on `127.0.0.1:2999`, which is the default connect address in vey-proxy.

### Listen sockets

Each listen socket is a `server` entry in the config file. A UDP server sets `listen` to an address.
A Unix datagram server sets `listen` to an absolute socket path. At least one server is required.

`-G` / `--group-name` sets the instance group and the control socket name. It does not select the listen address.

`vey-dcgen-ctl -G <group> reload` re-reads `server` and `issuer`. Listeners apply the difference in place. The CA certificate, private key, `keep_serial`, and `max_ttl` are swapped after those files load. A request already being signed keeps the previous CA. `duration_stats` stays as it was at startup.
`vey-dcgen-ctl -G <group> reload-issuer` re-reads only `issuer` and swaps the running CA. Listeners keep running.
`vey-dcgen-ctl -G <group> offline` stops listeners and lets in-flight requests finish.

## Command line options

Just run `vey-dcgen -h` to see all supported command line options.
