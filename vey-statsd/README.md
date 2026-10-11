[![docs](https://readthedocs.org/projects/vey-statsd/badge)](https://vey.readthedocs.io/projects/statsd/)

# VEY StatsD

`vey-statsd` is a [StatsD](https://github.com/statsd/statsd)-compatible
metrics ingestion and forwarding service.

It was built for the VEY project, where multiple applications emit metrics over
StatsD and need a lightweight service that can normalize, aggregate, and export
them to different downstream backends.

At a high level, `vey-statsd` lets you:

- accept StatsD-compatible metrics over UDP, or a Unix datagram socket
- process them through importers, collectors, and exporters
- aggregate or rewrite metrics before export
- forward metrics to multiple storage or observability systems

Key characteristics:

- async Rust implementation
- [DogStatsD](https://docs.datadoghq.com/developers/dogstatsd/datagram_shell/) tags (`#key:value`)
- sampled counters (`|@rate`) scaled by `1/rate`; a gauge keeps the value in the datagram
- separate emit intervals for collectors and exporters
- aggregation, including joining series after selected tags are dropped
- hot reload of the running process, or of one importer, collector, or exporter
- simple modular pipeline design

The current implementation is focused on VEY’s operational needs, but it is
usable as a standalone metrics relay as well.

## Architecture

`vey-statsd` is organized around three pipeline stages:

- `importer`
  Receives metrics from upstream senders and hands each record to a collector.

- `collector`
  Aggregates or rewrites incoming metrics, then forwards them to the next
  collector, to one or more exporters, or to both.

- `exporter`
  Sends processed metrics to downstream systems.

A missing collector or exporter name resolves to a discard sink, so a partial
pipeline still loads.

## Building

Set up the build environment first by following [dev-setup](../doc/dev-setup.md).

Build debug binaries:

```shell
cargo build -p vey-statsd -p vey-statsd-ctl
```

Build release binaries:

```shell
cargo build --profile release-lto -p vey-statsd -p vey-statsd-ctl
```

If you want to build binary packages or container images, see
[Build and Package](../doc/build_and_package.md).
How to run the published images is described in [docker/README.md](docker/README.md).

The main binaries are:

- `vey-statsd`: the metrics service
- `vey-statsd-ctl`: the local control and management CLI

## Documentation

The Sphinx-generated reference documentation is available on
[Read the Docs](https://vey.readthedocs.io/projects/statsd/en/latest/).

The configuration reference covers:

- importers
- collectors
- exporters
- runtime settings
- shared value types from the `vey-values` reference

## Supported Metric Types

The parser accepts:

- `c` — counter
- `g` — gauge

A histogram (`h`) or timer (`ms`) datagram is rejected.

A counter may carry a sample rate, as in `requests:1|c|@0.1`. The stored value
is `value / rate`. The rate must be in `(0, 1]`. A gauge is stored as written.

Tags use the DogStatsD suffix, as in `requests:1|c|#host:a,env:prod`.

## Pipeline Components

### Importers

- statsd

  Receives StatsD metrics and forwards them to a collector.
  UDP (`statsd` / `statsd_udp`) and, on Unix, a datagram socket (`statsd_unix`).

  `listen` sets the socket. `listen_in_worker` opens one socket in each worker
  runtime. `ingress_network_filter` limits which client addresses are accepted.

### Collectors

- aggregate

  Aggregates received metrics and emits the result on `emit_interval`
  (default 1s).
  Tags listed in `join_tags` are removed at emit time, and series that then
  share a name and the remaining tags are summed.

- regulate

  Rewrites received metrics and forwards them on without aggregating.
  `prefix` adds a common metric-name prefix.
  `drop_tags` removes tags from every metric.

- discard

  Drops every metric it receives.

Collectors may set `next` to chain another collector, and `exporter` to name
one or more exporters.

### Exporters

Graphite, OpenTSDB, Prometheus remote write, and both InfluxDB exporters
aggregate between their own `emit_interval` (default 10s) and accept a global
name prefix and static tags. Console, discard, and memory forward each sample
as it arrives.

| Exporter | What is sent | Backends |
| --- | --- | --- |
| console | Log each metric to stdout | local stdout |
| discard | Drop every metric | none |
| memory | Keep recent values in process memory | in-process |
| graphite | Graphite plaintext. A counter is its cumulative sum | Graphite, VictoriaMetrics |
| opentsdb | OpenTSDB `POST /api/put`. A counter is its cumulative sum | OpenTSDB, VictoriaMetrics |
| prometheus_push | Prometheus remote write 1.0. A counter is its cumulative sum. Dots in a StatsD name become `_` | Prometheus, VictoriaMetrics, Mimir, Cortex |
| influxdb_v2 | Line protocol via `POST /api/v2/write`. Counter fields are `count` (sum), `diff`, and `rate` | InfluxDB 2 and 3, VictoriaMetrics, QuestDB, OpenGemini |
| influxdb_v3 | Line protocol via `POST /api/v3/write_lp`, with the same counter fields | InfluxDB 3 over cleartext HTTP |

A counter that keeps receiving increments of `0` stays in the export. A counter
that receives no samples during an emit interval is omitted for that interval,
and the sum starts again on the next sample.

`influxdb_v2` always posts `/api/v2/write`. `opentsdb` always posts `/api/put`.
GreptimeDB and TDengine expect a prefixed write path
(`/v1/influxdb/api/v2/write`, `/v1/opentsdb/api/put`,
`/influxdb/v1/write`, `/opentsdb/v1/put/json/<db>`).

The configuration reference lists the default port for each exporter and the
query to use on each backend.

## Operations

`vey-statsd-ctl` talks to the local control socket. The default directory is
`/tmp/vey`, and the socket name is the daemon group (`-G`, default `default`).

```shell
vey-statsd-ctl -G default version
vey-statsd-ctl -G default reload
vey-statsd-ctl -G default list exporter
vey-statsd-ctl -G default reload-exporter graphite
```

| Command | Effect |
| --- | --- |
| `version` | Print the running daemon version |
| `reload` | Reload the process from its current config |
| `reload-importer`, `reload-collector`, `reload-exporter` | Reload one named object |
| `list importer\|collector\|exporter` | List configured objects of that kind |
| `offline` | Enter offline mode |
| `cancel-shutdown` | Cancel a shutdown that is still in its wait period |

## Typical Use Cases

- Collect application metrics over StatsD and forward them to Graphite,
  VictoriaMetrics, Prometheus, InfluxDB, QuestDB, OpenGemini, or OpenTSDB.
- Normalize names and tags before sending data downstream.
- Aggregate gauges or counters into a lower-cardinality stream.
- Run a lightweight metrics bridge for VEY services that already emit StatsD.

## Examples

Example configurations are available in [examples](examples):

- [export_console](examples/export_console/main.yaml) — UDP and Unix ingest, aggregate, print to stdout
- [export_graphite](examples/export_graphite/main.yaml)
- [export_opentsdb](examples/export_opentsdb/main.yaml)
- [export_prometheus_push](examples/export_prometheus_push/main.yaml)
- [export_influxdb](examples/export_influxdb/main.yaml)

## Getting Started

1. Set up the build or package environment with [dev-setup](../doc/dev-setup.md).
2. Build `vey-statsd` and `vey-statsd-ctl` as shown above.
3. Start from [examples/export_console](examples/export_console/main.yaml), which listens on `127.0.0.1:8125` and prints aggregated metrics to stdout.
4. Use the Sphinx reference when you need exact key names, ports, or backend queries.
5. For a published image, follow [docker/README.md](docker/README.md). The image listens on `0.0.0.0:8125` and prints to stdout.
