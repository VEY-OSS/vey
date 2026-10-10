# vey-statsd container image

Release tags `vey-statsd-v*` publish these images to GHCR:

| Tag | Dockerfile |
| --- | --- |
| `ghcr.io/vey-oss/vey-statsd:<version>` | `debian.Dockerfile` |
| `ghcr.io/vey-oss/vey-statsd:<version>-debian` | `debian.Dockerfile` |
| `ghcr.io/vey-oss/vey-statsd:<version>-alpine` | `alpine.Dockerfile` |

Debian is the default tag.

## Default config

The image starts `vey-statsd -c /etc/vey-statsd/ -G default -v` with [`config/main.yaml`](config/main.yaml):

- StatsD UDP on `0.0.0.0:8125`, so other containers can send metrics
- 10 second aggregation, joined on the `stat_id` tag
- aggregated metrics printed to stdout (`docker logs`)

The example configs under `examples/` listen on `127.0.0.1` or a Unix socket. Those addresses are not reachable from another container, so they are not what this image starts with.

The control socket is `/tmp/vey/default.sock`.

## Run

```shell
docker run --rm -d --name vey-statsd \
  -p 8125:8125/udp \
  ghcr.io/vey-oss/vey-statsd:<version>

echo "example.requests:1|c" | nc -u -w1 127.0.0.1 8125
docker logs vey-statsd
```

From another container on the same user-defined network, send to `vey-statsd:8125`. In `vey-proxy` that looks like:

```yaml
stat:
  target_udp:
    address: "@vey-statsd:8125"
```

A bare `host:port` string is accepted only as a literal IP address. The `@` prefix resolves the name once, when the config is loaded, and the name must return exactly one address.

Pass extra arguments after the image name to replace `CMD`:

```shell
docker run --rm ghcr.io/vey-oss/vey-statsd:<version> -V
```

## Use your own config

Mount a directory that contains `main.yaml` (or `vey-statsd.yaml`) over `/etc/vey-statsd`. Point `exporter` at the backend you want; the shipped config only prints to stdout.

```shell
docker run --rm -d --name vey-statsd \
  -p 8125:8125/udp \
  -v "$PWD/conf:/etc/vey-statsd:ro" \
  ghcr.io/vey-oss/vey-statsd:<version>
```

```shell
docker run --rm -v "$PWD/conf:/etc/vey-statsd:ro" \
  ghcr.io/vey-oss/vey-statsd:<version> -t -c /etc/vey-statsd/
```

## Control

```shell
docker exec vey-statsd vey-statsd-ctl -G default version
docker exec vey-statsd vey-statsd-ctl -G default reload
```

`-G` must match the daemon group. The default command uses `default`.

## Build locally

Run from the repository root:

```shell
docker build -f vey-statsd/docker/debian.Dockerfile . -t vey-statsd:local
docker build -f vey-statsd/docker/alpine.Dockerfile . -t vey-statsd:local-alpine
```
