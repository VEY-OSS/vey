#!/usr/bin/env python3
"""Query VictoriaMetrics after an OpenTSDB /api/put write."""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import phase

PREFIX = "vey.example"


def fetch(_names=None):
    matchers = [
        f'{{__name__="{PREFIX}.{name}",host="{phase.HOST}"}}' for name in phase.METRICS
    ]
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
        raw_name = metric.get("__name__", "")
        prefix = f"{PREFIX}."
        if not raw_name.startswith(prefix):
            continue
        name = raw_name[len(prefix):]
        if name not in pending:
            continue
        pending[name].extend(
            zip(item.get("timestamps") or [], item.get("values") or [])
        )
    for name, samples in pending.items():
        samples.sort()
        found[name] = [phase.point(value) for _, value in samples]
    return found, raw


if __name__ == "__main__":
    phase.run(fetch, timeout=40, label="victoriametrics opentsdb")
