#!/usr/bin/env python3
"""Query Prometheus query_range. Names are joined with underscores."""

import json
import sys
import time
import urllib.parse
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import phase

PREFIX = "vey_example_"


def collapse(points):
    kept = []
    for item in points:
        if not kept or not phase.close(kept[-1]["v"], item["v"]):
            kept.append(item)
    return kept


def fetch(_names=None):
    names = "|".join(f"{PREFIX}{name}" for name in phase.METRICS)
    query = urllib.parse.urlencode(
        {
            "query": f'{{__name__=~"{names}",host="{phase.HOST}"}}',
            "start": str(time.time() - 300),
            "end": str(time.time()),
            "step": "1s",
        }
    )
    raw = phase.http_get(f"http://127.0.0.1:9090/api/v1/query_range?{query}")
    data = json.loads(raw)
    found = {name: [] for name in phase.METRICS}
    if data.get("status") != "success":
        return found, raw
    for series in data.get("data", {}).get("result", []):
        metric = series.get("metric") or {}
        if metric.get("host") != phase.HOST:
            continue
        prom_name = metric.get("__name__", "")
        if not prom_name.startswith(PREFIX):
            continue
        name = prom_name[len(PREFIX):]
        if name not in found:
            continue
        samples = [(item[0], float(item[1])) for item in series.get("values") or []]
        samples.sort()
        found[name] = collapse([phase.point(value) for _, value in samples])
    return found, raw


if __name__ == "__main__":
    phase.run(fetch, timeout=40, label="prometheus")
