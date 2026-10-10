# vey-dcgen container image

Release tags `vey-dcgen-v*` publish these images to GHCR:

| Tag | Dockerfile |
| --- | --- |
| `ghcr.io/vey-oss/vey-dcgen:<version>` | `debian.Dockerfile` |
| `ghcr.io/vey-oss/vey-dcgen:<version>-debian` | `debian.Dockerfile` |
| `ghcr.io/vey-oss/vey-dcgen:<version>-alpine` | `alpine.Dockerfile` |

Debian is the default tag. The Alpine image links OpenSSL from the base image; the Debian image vendors it. Both images include `vey-mkcert`, which the entrypoint uses to create a CA.

## Default config

The image starts `vey-dcgen -c /etc/vey-dcgen/ -G default -v` with [`config/main.yaml`](config/main.yaml).

`vey-dcgen` cannot load without a CA certificate and private key. The example under `examples/simple` points at files that are not in the image. The container config points at `/var/lib/vey-dcgen/ca.crt` and `/var/lib/vey-dcgen/ca.key`. On first start, [`entrypoint.sh`](entrypoint.sh) runs `vey-mkcert --root --ec256` to create a P-256 root CA there when both files are missing. If only one of them exists, the entrypoint exits instead of overwriting it.

`UDP_LISTEN_ADDR` defaults to `0.0.0.0:2999`. The process otherwise listens on `127.0.0.1`, which other containers cannot reach. `-V` and `--help` skip CA generation.

A CA created in the writable container layer disappears when the container is removed. Mount a volume on `/var/lib/vey-dcgen` to keep it.

## Run

```shell
docker run --rm -d --name vey-dcgen \
  -p 2999:2999/udp \
  -v vey-dcgen-ca:/var/lib/vey-dcgen \
  ghcr.io/vey-oss/vey-dcgen:<version>

docker cp vey-dcgen:/var/lib/vey-dcgen/ca.crt ./ca.crt
```

`vey-proxy` connects to `127.0.0.1:2999` unless you set the peer. On a shared Docker network, point it at this container:

```yaml
auditor:
  - name: inspect
    tls_cert_generator:
      query_peer_addr: "@vey-dcgen:2999"
```

The `@` prefix resolves the name once at config load, and the name must return exactly one address. A bare hostname is rejected.

Install `ca.crt` on the clients that must trust intercepted certificates.

## Use your own CA

Mount both files. They must already be a matching certificate and private key.

```shell
docker run --rm -d --name vey-dcgen \
  -p 2999:2999/udp \
  -v "$PWD/ca.crt:/var/lib/vey-dcgen/ca.crt:ro" \
  -v "$PWD/ca.key:/var/lib/vey-dcgen/ca.key:ro" \
  ghcr.io/vey-oss/vey-dcgen:<version>
```

To change more than the CA paths, mount a directory that contains `main.yaml` over `/etc/vey-dcgen`.

Change the listen address with the environment variable:

```shell
docker run --rm -e UDP_LISTEN_ADDR=0.0.0.0:3000 -p 3000:3000/udp \
  ghcr.io/vey-oss/vey-dcgen:<version>
```

## Build locally

Run from the repository root:

```shell
docker build -f vey-dcgen/docker/debian.Dockerfile . -t vey-dcgen:local
docker build -f vey-dcgen/docker/alpine.Dockerfile . -t vey-dcgen:local-alpine
```
