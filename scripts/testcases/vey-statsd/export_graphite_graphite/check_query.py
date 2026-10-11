#!/usr/bin/env python3
"""Query Graphite's render API."""

import json
import sys
import urllib.parse
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import phase

PREFIX = "vey.example"


def fetch(_names=None):
    pairs = [("from", "-5min"), ("format", "json")]
    for name in phase.METRICS:
        pairs.append(("target", f"{PREFIX}.{name};host={phase.HOST}"))
    raw = phase.http_get(
        "http://127.0.0.1:8088/render?" + urllib.parse.urlencode(pairs)
    )
    found = {name: [] for name in phase.METRICS}
    for series in json.loads(raw):
        target = series.get("target", "")
        if "host=web" not in target:
            continue
        path = target.split(";", 1)[0]
        prefix = f"{PREFIX}."
        if not path.startswith(prefix):
            continue
        name = path[len(prefix):]
        if name not in found:
            continue
        samples = []
        for item in series.get("datapoints") or []:
            if item and item[0] is not None:
                samples.append((item[1], item[0]))
        samples.sort()
        found[name] = [phase.point(value) for _, value in samples]
    return found, raw


if __name__ == "__main__":
    phase.run(fetch, timeout=60, label="graphite")
