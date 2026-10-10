# vey-proxy container image

Release tags `vey-proxy-v*` publish these images to GHCR:

| Tag | Dockerfile |
| --- | --- |
| `ghcr.io/vey-oss/vey-proxy:<version>` | `debian.Dockerfile` |
| `ghcr.io/vey-oss/vey-proxy:<version>-debian` | `debian.Dockerfile` |
| `ghcr.io/vey-oss/vey-proxy:<version>-alpine` | `alpine.Dockerfile` |

Debian is the default tag. The Alpine image is smaller and links c-ares from the base image. The Debian image also contains `vey-proxy-ftp`. `lua.alpine.Dockerfile` is not published by CI; build it locally when you need `vey-proxy-lua`.

## Default config

The image starts `vey-proxy -c /etc/vey-proxy/ -G default -v` with [`config/main.yaml`](config/main.yaml):

- HTTP proxy on `0.0.0.0:8080`
- SOCKS5 proxy on `0.0.0.0:1080`
- direct egress through the container resolver (`/etc/resolv.conf`)
- IPv4 only, because many container networks have no working IPv6 route
- task and escape logs on stdout

There is no authentication and no egress filter. Treat this as a starting point. Do not publish `8080` or `1080` onto an untrusted network until you add a user group.

The control socket is `/tmp/vey/default.sock`.

## Run

```shell
docker run --rm -d --name vey-proxy \
  -p 8080:8080 -p 1080:1080 \
  ghcr.io/vey-oss/vey-proxy:<version>

curl -x http://127.0.0.1:8080 https://example.com/
curl --socks5-hostname 127.0.0.1:1080 https://example.com/
```

Pass extra arguments after the image name to replace `CMD`. The entrypoint remains `vey-proxy`.

```shell
docker run --rm ghcr.io/vey-oss/vey-proxy:<version> -V
```

## Use your own config

Mount a directory that contains `main.yaml` (or `vey-proxy.yaml`) over `/etc/vey-proxy`. Paths inside the config are resolved from that directory.

```shell
docker run --rm -d --name vey-proxy \
  -p 8080:8080 \
  -v "$PWD/conf:/etc/vey-proxy:ro" \
  ghcr.io/vey-oss/vey-proxy:<version>
```

Check the config without staying up:

```shell
docker run --rm -v "$PWD/conf:/etc/vey-proxy:ro" \
  ghcr.io/vey-oss/vey-proxy:<version> -t -c /etc/vey-proxy/
```

To send metrics to a `vey-statsd` container on the same Docker network, add this to `main.yaml`:

```yaml
stat:
  target_udp:
    address: "@vey-statsd:8125"
```

A bare `host:port` string is accepted only as a literal IP address. The `@` prefix resolves the name once, when the config is loaded, and the name must return exactly one address.

## Control

```shell
docker exec vey-proxy vey-proxy-ctl -G default version
docker exec vey-proxy vey-proxy-ctl -G default reload
docker exec vey-proxy vey-proxy-ctl -G default offline
```

`-G` must match the daemon group. The default command uses `default`.

## Build locally

Run from the repository root:

```shell
docker build -f vey-proxy/docker/debian.Dockerfile . -t vey-proxy:local
docker build -f vey-proxy/docker/alpine.Dockerfile . -t vey-proxy:local-alpine
docker build -f vey-proxy/docker/lua.alpine.Dockerfile . -t vey-proxy:local-lua
```
