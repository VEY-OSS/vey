Setup for vey-proxy coverage tests
==================================

# Install Required Tools

We use the following tools in the coverage scripts:

## docker-compose

We use docker containers to run various target services, i.e. httpbin.

Install on Debian:

```shell
apt install docker.io docker-compose
```

## systemd-resolved

We use systemd-resolved to add local dns records, and also use it as the target dns server.

Install on Debian:

```text
apt install systemd-resolved
```

# Setup local DNS

Add the following to `/etc/hosts` if you have systemd-resolved < 261:

```text
127.0.0.1 httpbin.local
127.0.0.1 vey-proxy.local
```

Add a static record if you have systemd-resolved >= 261.

```shell
cp resolved-ci-static.rr /etc/systemd/resolve/static.d/
```

# Run the Docker Containers

Start:

```shell
docker compose -f docker-compose.yml up -d
```

Stop:

```shell
docker compose -f docker-compose.yml down
```
