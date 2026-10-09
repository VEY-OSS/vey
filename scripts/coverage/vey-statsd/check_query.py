#!/usr/bin/env python3
"""Send StatsD samples in three phases and check each backend's stored series.

Phase 1 is one datagram, so every line shares one emit window:
  solo_c=4 and solo_g=2.5 are single points and must show up as themselves.
  batch_c is 1+2+4 and must be stored as one point, 7.
  batch_g is 1 then 9 then 2.5 and must be stored as one point, 2.5.
  span_c is 10+1 and span_g is 5. idle_c is 6.

Phase 2 is sent as soon as span_c's first point is visible. The exporter keeps
a series until the next tick, so the follow-up has to arrive inside that
window. Exporter emit_interval is 5s, and the gate polls span_c alone, so a
slow query does not consume the whole window.
  span_c += 2, so the series is 11 then 13.
  span_g becomes 5 then 8.
  Both points must remain. A single 13 means the first window was lost or the
  two windows were merged. A second count of 2 means the series was dropped
  before the follow-up arrived and the sum restarted.

Phase 3 waits out an idle exporter interval, then sends idle_c=1.
  The stored series is 6 then 1. The sum restarts. The first point stays.

InfluxDB also checks count, diff, and rate. rate is diff / emit_interval.
Graphite keeps the dotted name plus ;host=web. Prometheus joins the name with
_. OpenTSDB keeps the dotted name and the host tag.
"""

import json
import os
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

HOST = "web"
PREFIX = "vey.example"
EMIT_SECONDS = 5.0
COUNTERS = ("solo_c", "batch_c", "span_c", "idle_c")
GAUGES = ("solo_g", "batch_g", "span_g")
METRICS = COUNTERS + GAUGES

PHASE1 = [
    "solo_c:4|c|#host:web",
    "solo_g:2.5|g|#host:web",
    "batch_c:1|c|#host:web",
    "batch_c:2|c|#host:web",
    "batch_c:4|c|#host:web",
    "batch_g:1|g|#host:web",
    "batch_g:9|g|#host:web",
    "batch_g:2.5|g|#host:web",
    "span_c:10|c|#host:web",
    "span_c:1|c|#host:web",
    "span_g:5|g|#host:web",
    "idle_c:6|c|#host:web",
]
PHASE2 = [
    "span_c:2|c|#host:web",
    "span_g:8|g|#host:web",
]
PHASE3 = ["idle_c:1|c|#host:web"]

def ctr(count, diff):
    return (count, diff, diff / EMIT_SECONDS)


# (count, diff, rate) for counters. Gauge lists are the stored values.
EXPECT_PHASE1 = {
    "solo_c": [ctr(4, 4)],
    "solo_g": [2.5],
    "batch_c": [ctr(7, 7)],
    "batch_g": [2.5],
    "span_c": [ctr(11, 11)],
    "span_g": [5],
    "idle_c": [ctr(6, 6)],
}
EXPECT_PHASE2 = {
    **EXPECT_PHASE1,
    "span_c": [ctr(11, 11), ctr(13, 2)],
    "span_g": [5, 8],
}
EXPECT_PHASE3 = {
    **EXPECT_PHASE2,
    "idle_c": [ctr(6, 6), ctr(1, 1)],
}


def http_read(req):
    try:
        with urllib.request.urlopen(req, timeout=5) as resp:
            return resp.read().decode()
    except urllib.error.HTTPError as exc:
        return exc.read().decode(errors="replace")
    except urllib.error.URLError:
        return ""


def http_get(url):
    return http_read(urllib.request.Request(url))


def http_post(url, payload, headers):
    req = urllib.request.Request(url, data=payload, headers=headers, method="POST")
    return http_read(req)


def close(value, expected):
    try:
        return abs(float(value) - expected) < 1e-6
    except (TypeError, ValueError):
        return False


def wait_listen():
    for _ in range(50):
        try:
            out = subprocess.check_output(["ss", "-lunH"], text=True)
        except (subprocess.CalledProcessError, FileNotFoundError):
            out = ""
        if "127.0.0.1:8125" in out:
            return
        time.sleep(0.1)
    sys.exit("statsd is not listening on 127.0.0.1:8125")


def send(lines):
    payload = "".join(f"{line}\n" for line in lines).encode()
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.sendto(payload, ("127.0.0.1", 8125))
    sock.close()


def collapse(points):
    kept = []
    for point in points:
        if not kept or not close(kept[-1]["v"], point["v"]):
            kept.append(point)
    return kept


def point(value):
    return {"v": float(value)}


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


def parse_graphite(raw):
    found = {name: [] for name in METRICS}
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
        found[name] = [point(value) for _, value in samples]
    return found


def parse_influx_body(raw, gauge):
    points = []
    for row in rows_from_json(raw):
        if gauge:
            if "value" not in row:
                continue
            points.append(point(row["value"]))
            continue
        if "count" not in row:
            continue
        item = point(row["count"])
        item["diff"] = float(row["diff"])
        item["rate"] = float(row["rate"])
        points.append(item)
    return points


def parse_prometheus(raw):
    data = json.loads(raw)
    if data.get("status") != "success":
        return {}
    found = {name: [] for name in METRICS}
    for series in data.get("data", {}).get("result", []):
        metric = series.get("metric") or {}
        if metric.get("host") != HOST:
            continue
        prom_name = metric.get("__name__", "")
        prefix = "vey_example_"
        if not prom_name.startswith(prefix):
            continue
        name = prom_name[len(prefix):]
        if name not in found:
            continue
        samples = []
        for item in series.get("values") or []:
            samples.append((item[0], float(item[1])))
        samples.sort()
        found[name] = collapse([point(value) for _, value in samples])
    return found


def parse_opentsdb(raw):
    found = {name: [] for name in METRICS}
    pending = {name: [] for name in METRICS}
    for line in raw.splitlines():
        line = line.strip()
        if not line:
            continue
        item = json.loads(line)
        metric = item.get("metric") or {}
        if metric.get("host") != HOST:
            continue
        raw_name = metric.get("__name__", "")
        prefix = f"{PREFIX}."
        if not raw_name.startswith(prefix):
            continue
        name = raw_name[len(prefix):]
        if name not in pending:
            continue
        timestamps = item.get("timestamps") or []
        values = item.get("values") or []
        pending[name].extend(zip(timestamps, values))
    for name, samples in pending.items():
        samples.sort()
        found[name] = [point(value) for _, value in samples]
    return found


def graphite_url():
    pairs = [("from", "-5min"), ("format", "json")]
    for name in METRICS:
        pairs.append(("target", f"{PREFIX}.{name};host={HOST}"))
    return "http://127.0.0.1:8088/render?" + urllib.parse.urlencode(pairs)


def influx_sql(name, gauge):
    table = f'{PREFIX}.{name}'
    if gauge:
        selected = '"value"'
    else:
        selected = '"count" AS count, "diff" AS diff, "rate" AS rate'
    return (
        f'SELECT {selected} FROM "{table}" WHERE host = \'{HOST}\' ORDER BY time'
    )


def fetch_influx(db, names=None):
    token = os.environ["INFLUXDB3_AUTH_TOKEN"]
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json",
    }
    found = {}
    bodies = []
    for name in names or METRICS:
        gauge = name in GAUGES
        payload = json.dumps(
            {"db": db, "q": influx_sql(name, gauge), "format": "json"}
        ).encode()
        raw = http_post("http://127.0.0.1:8181/api/v3/query_sql", payload, headers)
        bodies.append(raw)
        try:
            found[name] = parse_influx_body(raw, gauge)
        except (json.JSONDecodeError, KeyError, TypeError, ValueError):
            found[name] = []
    return found, "\n".join(bodies)


def fetch(kind, db, names=None):
    if kind == "graphite":
        raw = http_get(graphite_url())
        return parse_graphite(raw), raw
    if kind == "influx":
        return fetch_influx(db, names)
    if kind == "prometheus":
        names = "|".join(f"vey_example_{name}" for name in METRICS)
        query = urllib.parse.urlencode(
            {
                "query": f'{{__name__=~"{names}",host="{HOST}"}}',
                "start": str(time.time() - 300),
                "end": str(time.time()),
                "step": "1s",
            }
        )
        raw = http_get(f"http://127.0.0.1:9090/api/v1/query_range?{query}")
        return parse_prometheus(raw), raw
    if kind == "opentsdb":
        start = str(int(time.time()) - 3600)
        pairs = [("start", start)]
        for name in METRICS:
            pairs.append(
                ("match[]", f'{{__name__="{PREFIX}.{name}",host="{HOST}"}}')
            )
        raw = http_get(
            "http://127.0.0.1:8428/api/v1/export?" + urllib.parse.urlencode(pairs)
        )
        return parse_opentsdb(raw), raw
    sys.exit(f"unknown backend {kind}")


def snapshot(found):
    view = {}
    for name, points in found.items():
        if name in COUNTERS and points and "diff" in points[0]:
            view[name] = [
                (point["v"], point["diff"], point["rate"]) for point in points
            ]
        else:
            view[name] = [point["v"] for point in points]
    return view


def matches(found, kind, expected):
    for name, want in expected.items():
        points = found.get(name) or []
        if kind == "influx" and name in COUNTERS:
            if len(points) != len(want):
                return False
            for point, (count, diff, rate) in zip(points, want):
                if "diff" not in point or "rate" not in point:
                    return False
                if not (
                        close(point["v"], count)
                        and close(point["diff"], diff)
                        and close(point["rate"], rate)
                        and close(point["rate"], point["diff"] / EMIT_SECONDS)
                ):
                    return False
            continue
        values = [item[0] if isinstance(item, tuple) else item for item in want]
        if len(points) != len(values):
            return False
        if not all(close(point["v"], value) for point, value in zip(points, values)):
            return False
    return True


def wait_for(kind, db, expected, timeout, names=None):
    deadline = time.time() + timeout
    last = ""
    found = {}
    while time.time() < deadline:
        try:
            found, last = fetch(kind, db, names)
        except (json.JSONDecodeError, KeyError, IndexError, TypeError, ValueError):
            found = {}
        if matches(found, kind, expected):
            return found
        time.sleep(0.2)
    sys.stderr.write(
        f"{kind} query failed\nwant={expected}\nfound={snapshot(found)}\n{last[-4000:]}\n"
    )
    sys.exit(1)


def main():
    kind = sys.argv[1]
    db = sys.argv[2] if len(sys.argv) > 2 else ""
    if kind not in ("graphite", "influx", "prometheus", "opentsdb"):
        sys.exit(f"unknown backend {kind}")
    if kind == "influx" and not db:
        sys.exit("influx check needs a database name")

    timeout = 60 if kind == "graphite" else 40
    wait_listen()

    send(PHASE1)
    # One series, so the follow-up is sent while the exporter still holds it.
    wait_for(kind, db, {"span_c": [ctr(11, 11)]}, timeout, names=["span_c"])
    send(PHASE2)
    wait_for(kind, db, EXPECT_PHASE2, timeout)
    print(f"ok {kind} single-point and same-window aggregate")
    print(f"ok {kind} cross-window aggregate")

    time.sleep(EMIT_SECONDS + 1)
    send(PHASE3)
    wait_for(kind, db, EXPECT_PHASE3, timeout)
    print(f"ok {kind} idle sum restart")


if __name__ == "__main__":
    main()
