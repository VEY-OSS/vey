#!/usr/bin/env python3
"""Query InfluxDB 3 SQL for the v2 write path. Counters include diff and rate."""

import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import phase

PREFIX = "vey.example"
DATABASE = "statsdv2"


def rows_from_json(raw):
    data = json.loads(raw)
    if isinstance(data, list):
        return [row for row in data if isinstance(row, dict)]
    rows = []
    for result in data.get("results") or []:
        for series in result.get("series") or []:
            columns = series.get("columns") or []
            for values in series.get("values") or []:
                rows.append(dict(zip(columns, values)))
    return rows


def sql(name, gauge):
    table = f"{PREFIX}.{name}"
    if gauge:
        selected = '"value"'
    else:
        selected = '"count" AS count, "diff" AS diff, "rate" AS rate'
    return f'SELECT {selected} FROM "{table}" WHERE host = \'{phase.HOST}\' ORDER BY time'


def fetch(names=None):
    token = os.environ["INFLUXDB3_AUTH_TOKEN"]
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json",
    }
    found = {}
    bodies = []
    for name in names or phase.METRICS:
        gauge = name in phase.GAUGES
        payload = json.dumps(
            {"db": DATABASE, "q": sql(name, gauge), "format": "json"}
        ).encode()
        raw = phase.http_post(
            "http://127.0.0.1:8181/api/v3/query_sql", payload, headers
        )
        bodies.append(raw)
        points = []
        for row in rows_from_json(raw):
            if gauge:
                if "value" not in row:
                    continue
                points.append(phase.point(row["value"]))
                continue
            if "count" not in row:
                continue
            item = phase.point(row["count"])
            item["diff"] = float(row["diff"])
            item["rate"] = float(row["rate"])
            points.append(item)
        found[name] = points
    return found, "\n".join(bodies)


if __name__ == "__main__":
    phase.run(fetch, timeout=40, label="influxdb_v2", influx=True)
