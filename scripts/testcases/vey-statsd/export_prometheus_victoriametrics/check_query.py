#!/usr/bin/env python3
"""Query VictoriaMetrics after a Prometheus remote-write."""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import phase

PREFIX = "vm_prom_"


def fetch(_names=None):
    names = "|".join(f"{PREFIX}{name}" for name in phase.METRICS)
    raw = phase.vm_export([f'{{__name__=~"{names}",host="{phase.HOST}"}}'])
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
        prom_name = metric.get("__name__", "")
        if not prom_name.startswith(PREFIX):
            continue
        name = prom_name[len(PREFIX):]
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
    phase.run(fetch, timeout=40, label="victoriametrics prometheus")
