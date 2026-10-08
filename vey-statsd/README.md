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
- compatible with [DogStatsD](https://docs.datadoghq.com/developers/dogstatsd/datagram_shell/) tags
- separate emit intervals for exporters
- support for aggregation and tag dropping
- simple modular pipeline design

The current implementation is focused on VEY’s operational needs, but it is
usable as a standalone metrics relay as well.

## Architecture

`vey-statsd` is organized around three pipeline stages:

- `importer`
  Receives metrics from upstream senders.

- `collector`
  Aggregates or rewrites incoming metrics.

- `exporter`
  Sends processed metrics to downstream systems.

This structure makes it easy to build either a minimal relay or a multi-stage
metrics pipeline.

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

- `c` - counter
- `g` - gauge
- `h` - histogram, not yet supported
- `ms` - timer, not yet supported

## Pipeline Components

### Importers

- statsd

  Receives StatsD metrics and forwards them to collectors.
  UDP (`statsd` / `statsd_udp`) and, on Unix, a datagram socket (`statsd_unix`).

### Collectors

- aggregate

  Aggregates received metrics and emits the result to exporters.
  `join_tags` can be used to merge series after dropping selected tags.

- regulate

  Rewrites received metrics and forwards them directly to exporters.
  Supported actions include:
  `prefix` to add a common metric-name prefix
  `drop_tags` to remove tags from all metrics

### Exporters

| Exporter    | What is sent                                                                 | Backends                                              | Aggregate | Global prefix and tags |
|-------------|------------------------------------------------------------------------------|-------------------------------------------------------|-----------|------------------------|
| console     | Log all metrics to stdout                                                    | local stdout                                          | no        | no                     |
| discard     | Discard all metrics                                                          | none                                                  | no        | no                     |
| memory      | Store all metric values in memory                                            | in-process                                            | no        | no                     |
| graphite    | Graphite plaintext. A counter is its cumulative sum                          | Graphite, VictoriaMetrics                                 | yes       | yes                    |
| opentsdb    | OpenTSDB `/api/put`. A counter value is its cumulative sum                   | OpenTSDB, VictoriaMetrics                             | yes       | yes                    |
| influxdb_v2 | Line protocol via `/api/v2/write`. Counter fields are `count` (sum), `diff`, and `rate` | InfluxDB 2 and 3, VictoriaMetrics, QuestDB, OpenGemini | yes | yes |
| influxdb_v3 | Line protocol via `/api/v3/write_lp`, with the same counter fields           | InfluxDB 3 over cleartext HTTP                        | yes       | yes                    |

The configuration reference lists the listen port, the query for each backend,
and the paths this exporter does not send. GreptimeDB and TDengine use a
prefixed write URL.

## Typical Use Cases

- Collect application metrics over StatsD and forward them to Graphite,
  VictoriaMetrics, InfluxDB, QuestDB, OpenGemini, or OpenTSDB.
- Normalize names and tags before sending data downstream.
- Aggregate gauges or counters into a lower-cardinality stream.
- Run a lightweight metrics bridge for VEY services that already emit StatsD.

## Examples

Example configurations are available in [examples](examples).
