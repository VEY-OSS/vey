#!/usr/bin/env python3
"""Query VictoriaMetrics after an InfluxDB v2 line-protocol write.

A gauge field is stored as measurement_value. Counter fields are separate
series; this check follows measurement_count.
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import phase

PREFIX = "vm.influx"


def series_name(raw_name):
    prefix = f"{PREFIX}."
    if not raw_name.startswith(prefix):
        return None
    rest = raw_name[len(prefix):]
    if rest.endswith("_diff") or rest.endswith("_rate"):
        return None
    if rest.endswith("_count"):
        rest = rest[: -len("_count")]
    elif rest.endswith("_value"):
        rest = rest[: -len("_value")]
    return rest if rest in phase.METRICS else None


def fetch(_names=None):
    matchers = []
    for name in phase.METRICS:
        metric = f"{PREFIX}.{name}"
        if name in phase.COUNTERS:
            metric = f"{metric}_count"
        else:
            metric = f"{metric}_value"
        matchers.append(f'{{__name__="{metric}",host="{phase.HOST}"}}')
    raw = phase.vm_export(matchers)
    found = {name: [] for name in phase.METRICS}
    pending = {name: [] for name in phase.METRICS}
    for line in raw.splitlines():
        line = line.strip()
        if not line:
            continue
        item = json.loads(line)
        metric = item.get("metric") or {}
        if metric.get("host") != phase.HOST:
            continue
        name = series_name(metric.get("__name__", ""))
        if name is None:
            continue
        pending[name].extend(
            zip(item.get("timestamps") or [], item.get("values") or [])
        )
    for name, samples in pending.items():
        samples.sort()
        found[name] = [phase.point(value) for _, value in samples]
    return found, raw


if __name__ == "__main__":
    phase.run(fetch, timeout=40, label="victoriametrics influx")
