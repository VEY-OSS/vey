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
cargo build -p vey-dcgen -p vey-dcgen-ctl -p vey-dcgen-get
```

To build release binaries:

```shell
cargo build --profile release-lto -p vey-dcgen -p vey-dcgen-ctl -p vey-dcgen-get
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

`vey-dcgen-ctl -G <group> reload` re-reads `server`, `issuer`, and `site`. Listeners apply the difference in place. The CA certificate, private key, predefined site certificates, `keep_serial`, and `max_ttl` are swapped after those files load. A request already being signed keeps the previous CA. `duration_stats` rebuilds the issue-duration histogram. A site loaded from a file records that file.
`vey-dcgen-ctl -G <group> reload-issuer` re-reads `issuer` from the main config and swaps the running CA. Listeners keep running. Site certificates stay as they are.
`vey-dcgen-ctl -G <group> reload-site <SITE-ID>` re-reads the recorded file for that site. The new `id` must match `SITE-ID`.
`vey-dcgen-ctl -G <group> offline` stops listeners and lets in-flight requests finish.

`vey-dcgen-get` asks a UDP or Unix datagram listener for one host. The reply prints the TTL, certificate chain, and private key. A host with no reply is printed as `not found`.

```shell
vey-dcgen-get www.example.com
vey-dcgen-get --udp 127.0.0.1:2999 www.example.com
vey-dcgen-get --unix /run/vey-dcgen/dcgen.sock www.example.com
```

## Command line options

Just run `vey-dcgen -h` to see all supported command line options.
