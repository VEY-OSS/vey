# Migration from G3 to VEY

VEY is a continuation of G3 by the original author.

The overall architecture and most configuration concepts remain close to G3, so
this is not a ground-up redesign migration. The main breaking change is naming:
the applications, package names, systemd units, runtime directories, and
configuration directories have all moved from the old G3 names to VEY names.

That means migration is mostly operational rather than conceptual. You will
usually spend more time updating service management, deployment scripts, and
filesystem paths than rewriting configuration content.

## Component renames

The current VEY tree shows these daemon renames:

| G3 name    | VEY name      | Typical role |
|------------|---------------|--------------|
| `g3proxy`  | `vey-proxy`   | forward/transparent/stream proxy |
| `g3keymess`| `vey-keyless` | keyless TLS service |
| `g3statsd` | `vey-statsd`  | StatsD-compatible metrics service |
| `g3tiles`  | `vey-gateway` | reverse proxy / gateway daemon |
| `g3iploc`  | `vey-iploc`   | IP location helper service |
| `g3fcgen`  | `vey-dcgen`   | dynamic certificate generator |

The corresponding packaged systemd unit names follow the same pattern:

| Old unit             | New unit               |
|----------------------|------------------------|
| `g3proxy@.service`   | `vey-proxy@.service`   |
| `g3keymess@.service` | `vey-keyless@.service` |
| `g3statsd@.service`  | `vey-statsd@.service`  |
| `g3tiles@.service`   | `vey-gateway@.service` |
| `g3iploc@.service`   | `vey-iploc@.service`   |
| `g3fcgen@.service`   | `vey-dcgen@.service`   |

The packaged configuration directories also follow the same rename:

| Old config dir       | New config dir         |
|----------------------|------------------------|
| `/etc/g3proxy/`      | `/etc/vey-proxy/`      |
| `/etc/g3keymess/`    | `/etc/vey-keyless/`    |
| `/etc/g3statsd/`     | `/etc/vey-statsd/`     |
| `/etc/g3tiles/`      | `/etc/vey-gateway/`    |
| `/etc/g3iploc/`      | `/etc/vey-iploc/`      |
| `/etc/g3fcgen/`      | `/etc/vey-dcgen/`      |

The packaged runtime directories used by the shipped systemd services are:

| VEY service          | RuntimeDirectory       |
|----------------------|------------------------|
| `vey-proxy`          | `/run/vey-proxy/`      |
| `vey-keyless`        | `/run/vey-keyless/`    |
| `vey-statsd`         | `/run/vey-statsd/`     |
| `vey-gateway`        | `/run/vey-gateway/`    |

`vey-iploc` and `vey-dcgen` do not currently declare a `RuntimeDirectory` in
their packaged systemd templates, so if your old deployment used explicit state
or socket paths for those services, review them manually.

## Control and helper commands

For services that expose a local controller, the VEY command names also use the
new prefix:

| Old command        | New command          |
|--------------------|----------------------|
| `g3proxy-ctl`      | `vey-proxy-ctl`      |
| `g3keymess-ctl`    | `vey-keyless-ctl`    |
| `g3statsd-ctl`     | `vey-statsd-ctl`     |
| `g3tiles-ctl`      | `vey-gateway-ctl`    |

The proxy utility binaries are renamed as well:

| Old command        | New command        |
|--------------------|--------------------|
| `g3proxy-lua`      | `vey-proxy-lua`    |
| `g3proxy-ftp`      | `vey-proxy-ftp`    |

## What usually does not need rewriting

In most deployments, the following areas should remain broadly familiar:

- high-level configuration structure
- server, escaper, resolver, auth, and auditor concepts
- most YAML field names
- controller and hot-reload workflow
- metrics and logging concepts

You should still review release notes and service-specific docs before
switching production traffic, but the migration is usually closer to a rename
and packaging transition than to a complete reconfiguration.

## What usually does need updating

You should expect to update:

- binary names in service files and scripts
- systemd unit names
- configuration paths under `/etc`
- runtime paths under `/run`
- package names in install/upgrade automation
- default metrics prefixes if you relied on the built-in service name
- log paths, PID paths, socket paths, and state paths if they include old names
- monitoring and alerting rules that match service or process names
- container entrypoints and image tags if they refer to old G3 binaries

## Metrics prefix changes

The default StatsD prefix has changed along with the service names.

If you relied on the built-in prefix instead of setting your own explicit
``prefix`` in the ``stat`` section, you should expect metric names to change.

Known defaults in the current VEY tree include:

| Old default prefix | New default prefix |
|--------------------|--------------------|
| `g3proxy`          | `vey-proxy`        |
| `g3keymess`        | `vey-keyless`      |
| `g3tiles`          | `vey-gateway`      |

At minimum, review:

- dashboards
- alert rules
- recording rules
- metric relabeling rules
- log-to-metric pipelines
- any scripts that query metrics by name prefix

If you want to avoid a metrics-name migration during cutover, set an explicit
``prefix`` in the VEY ``stat`` configuration to keep emitting the old G3
prefix temporarily.

## Suggested migration procedure

The safest migration path is side-by-side preparation followed by a controlled
cutover.

1. Install VEY binaries or packages without deleting your G3 configuration yet.
2. Copy the G3 configuration tree to the matching VEY location.
3. Update service files, env files, and scripts to use VEY binary names and
   VEY paths.
4. Review any hard-coded references to old G3 names under `/etc`, `/run`,
   `/var/lib`, `/var/log`, cron jobs, deployment tooling, and observability
   configs.
5. Start VEY in a staging environment or with production traffic still drained.
6. Verify listener ports, controller access, logs, metrics, and reload
   behavior.
7. Stop the G3 service instances.
8. Enable and start the corresponding VEY service instances.
9. Re-run operational checks after cutover.

## Example systemd migration

If you previously used a templated `g3proxy` systemd service instance such as:

```text
g3proxy@main.service
```

the VEY equivalent is:

```text
vey-proxy@main.service
```

The packaged VEY service template uses:

```text
EnvironmentFile=-/etc/vey-proxy/%i/env
ExecStart=/usr/bin/vey-proxy -c /etc/vey-proxy/%i/ --control-dir /run/vey-proxy -s -G %i
ExecStop=/usr/bin/vey-proxy-ctl --control-dir /run/vey-proxy -G %i -p $MAINPID offline
```

So the matching migration for one instance is usually:

- `/etc/g3proxy/main/` -> `/etc/vey-proxy/main/`
- `/etc/g3proxy/main/env` -> `/etc/vey-proxy/main/env`
- `g3proxy@main.service` -> `vey-proxy@main.service`
- `g3proxy-ctl` -> `vey-proxy-ctl`
- old control dir under `/run/g3proxy/` -> `/run/vey-proxy/`

The same pattern applies to `g3keymess`, `g3statsd`, `g3tiles`, `g3iploc`, and
`g3fcgen`.

## Practical checklist

Before the final cutover, search your deployment for old names such as:

- `g3proxy`
- `g3keymess`
- `g3statsd`
- `g3tiles`
- `g3iploc`
- `g3fcgen`
- `/etc/g3`
- `/run/g3`

Typical places to check:

- systemd unit overrides
- environment files
- Ansible roles
- shell scripts
- Dockerfiles and container entrypoints
- Kubernetes manifests
- monitoring rules and dashboards
- log collection rules
- StatsD prefix rewrites or downstream aggregation rules

## Verification after cutover

After starting VEY services, verify at least:

- the expected VEY systemd units are active
- the daemon reads config from the new `/etc/vey-*` path
- control commands use the new `vey-*-ctl` binary names
- metrics and logs are still reaching their downstream systems
- metric names use the prefix you expect
- reload and offline actions still work
- any helper service references, such as `vey-iploc` from `vey-proxy`, use the
  updated service names and paths

## Backward compatibility for configuration loading

If you point the `-c` option to a directory instead of a specific file, the
daemon will automatically search for and load the main configuration file based
on the executable binary name.

This means you can create a symbolic link from a VEY binary to its old G3
name (e.g., `ln -s /usr/bin/vey-proxy /usr/bin/g3proxy`). When you execute the
daemon via the `g3proxy` symlink and specify a directory with `-c` (e.g.,
`/etc/g3proxy/`), it will automatically look for `g3proxy.yaml` in that
directory. This allows you to load old config file directories easily.

## Notes

Some daemon option parsers in VEY still recognize the historical G3 program
names internally for compatibility purposes. That should be treated as a
transition aid, not as the target deployment model. New packaging, service
definitions, scripts, and documentation should use the VEY names consistently.
