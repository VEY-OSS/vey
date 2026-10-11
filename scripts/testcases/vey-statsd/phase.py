"""Three-phase StatsD samples shared by each exporter check.

Phase 1 is one datagram, so every line shares one emit window:
  solo_c=4 and solo_g=2.5 are single points and must show up as themselves.
  batch_c is 1+2+4 and must be stored as one point, 7.
  batch_g is 1 then 9 then 2.5 and must be stored as one point, 2.5.
  span_c is 10+1 and span_g is 5. idle_c is 6.

Phase 2 is sent as soon as span_c's first point is visible. The exporter keeps
a series until the next tick, so the follow-up has to arrive inside that
window. Exporter emit_interval is 5s.
  span_c += 2, so the series is 11 then 13.
  span_g becomes 5 then 8.
  Both points must remain. A single 13 means the first window was lost or the
  two windows were merged. A second count of 2 means the series was dropped
  before the follow-up arrived and the sum restarted.

Phase 3 waits out an idle exporter interval, then sends idle_c=1.
  The stored series is 6 then 1. The sum restarts. The first point stays.
"""

import json
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

HOST = "web"
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


def vm_export(matchers):
    """Flush VictoriaMetrics, then export the given series selectors."""
    try:
        urllib.request.urlopen(
            "http://127.0.0.1:8428/internal/force_flush", timeout=5
        ).read()
    except urllib.error.URLError:
        pass
    pairs = [("start", str(int(time.time()) - 3600))]
    for matcher in matchers:
        pairs.append(("match[]", matcher))
    return http_get(
        "http://127.0.0.1:8428/api/v1/export?" + urllib.parse.urlencode(pairs)
    )


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


def point(value):
    return {"v": float(value)}


def snapshot(found):
    view = {}
    for name, points in found.items():
        if name in COUNTERS and points and "diff" in points[0]:
            view[name] = [
                (item["v"], item["diff"], item["rate"]) for item in points
            ]
        else:
            view[name] = [item["v"] for item in points]
    return view


def matches(found, expected, influx):
    for name, want in expected.items():
        points = found.get(name) or []
        if influx and name in COUNTERS:
            if len(points) != len(want):
                return False
            for item, (count, diff, rate) in zip(points, want):
                if "diff" not in item or "rate" not in item:
                    return False
                if not (
                    close(item["v"], count)
                    and close(item["diff"], diff)
                    and close(item["rate"], rate)
                    and close(item["rate"], item["diff"] / EMIT_SECONDS)
                ):
                    return False
            continue
        values = [item[0] if isinstance(item, tuple) else item for item in want]
        if len(points) != len(values):
            return False
        if not all(close(item["v"], value) for item, value in zip(points, values)):
            return False
    return True


def wait_for(fetch, expected, timeout, influx, names=None):
    deadline = time.time() + timeout
    last = ""
    found = {}
    while time.time() < deadline:
        try:
            found, last = fetch(names)
        except (json.JSONDecodeError, KeyError, IndexError, TypeError, ValueError):
            found = {}
        if matches(found, expected, influx):
            return found
        time.sleep(0.2)
    sys.stderr.write(
        f"query failed\nwant={expected}\nfound={snapshot(found)}\n{last[-4000:]}\n"
    )
    sys.exit(1)


def run(fetch, timeout, label, influx=False):
    wait_listen()

    send(PHASE1)
    wait_for(fetch, {"span_c": [ctr(11, 11)]}, timeout, influx, names=["span_c"])
    send(PHASE2)
    wait_for(fetch, EXPECT_PHASE2, timeout, influx)
    print(f"ok {label} single-point and same-window aggregate")
    print(f"ok {label} cross-window aggregate")

    time.sleep(EMIT_SECONDS + 1)
    send(PHASE3)
    wait_for(fetch, EXPECT_PHASE3, timeout, influx)
    print(f"ok {label} idle sum restart")
