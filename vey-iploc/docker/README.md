# vey-iploc container image

Release tags `vey-iploc-v*` publish these images to GHCR. Tags and digests are listed on the [GitHub Packages page](https://github.com/VEY-OSS/vey/pkgs/container/vey-iploc).

| Tag | Dockerfile |
| --- | --- |
| `ghcr.io/vey-oss/vey-iploc:<version>` | `debian.Dockerfile` |
| `ghcr.io/vey-oss/vey-iploc:<version>-debian` | `debian.Dockerfile` |
| `ghcr.io/vey-oss/vey-iploc:<version>-alpine` | `alpine.Dockerfile` |

Debian is the default tag. Each published tag is a manifest list for linux/amd64 and linux/arm64.

## Default config

The image starts `vey-iploc -c /etc/vey-iploc/ -v` with [`config/main.yaml`](config/main.yaml).

That config listens on UDP `0.0.0.0:2888`. Change the `server` entry to use another address.

[`config/country.csv`](config/country.csv) is a placeholder that maps only `1.1.1.0/24` to `US`. It exists so the process can start. Replace it with a VEY country database before using the result for routing. See the [GeoIP Database](../README.md#geoip-database) section for how to convert MaxMind or IPinfo data.

## Run

```shell
docker run --rm -d --name vey-iploc \
  -p 2888:2888/udp \
  -v "$PWD/vey-country.csv:/etc/vey-iploc/country.csv:ro" \
  ghcr.io/vey-oss/vey-iploc:<version>
```

Mount an ASN database by replacing `main.yaml` as well, for example:

```yaml
server:
  - name: local
    type: udp
    listen: 0.0.0.0:2888

geoip_db:
  country: country.csv
  asn: asn.csv
```

```shell
docker run --rm -d --name vey-iploc \
  -p 2888:2888/udp \
  -v "$PWD/conf:/etc/vey-iploc:ro" \
  ghcr.io/vey-oss/vey-iploc:<version>
```

`vey-proxy` connects to `127.0.0.1:2888` unless you set the peer. On a shared Docker network, set it on the escaper that queries this service:

```yaml
escaper:
  - name: route_geoip
    type: route_geoip
    resolver: default
    ip_locate_service:
      query_peer_addr: "@vey-iploc:2888"
    geo_rules:
      - next: internet
        countries: US
    default_next: deny
```

`internet` and `deny` are other escapers in the same file. See `examples/escaper_route_geoip`. The `@` prefix resolves the name once at config load, and the name must return exactly one address. A bare hostname is rejected.

Change the listen port by mounting a config whose `server` entry uses that port, and publish the same UDP port.

## Build locally

Run from the repository root:

```shell
docker build -f vey-iploc/docker/debian.Dockerfile . -t vey-iploc:local
docker build -f vey-iploc/docker/alpine.Dockerfile . -t vey-iploc:local-alpine
```
