#!/usr/bin/env python3
"""Wait until a Graphite carbon port stays open after a plaintext write."""

import argparse
import json
import socket
import time
import urllib.error
import urllib.parse
import urllib.request

READINESS_METRIC = "vey.readiness"


def accept_line(host: str, port: int) -> str:
    try:
        sock = socket.create_connection((host, port), timeout=2)
    except OSError as exc:
        return str(exc)
    try:
        sock.sendall(f"{READINESS_METRIC} 1 {int(time.time())}\n".encode())
        sock.settimeout(1)
        try:
            data = sock.recv(16)
        except TimeoutError:
            return ""
        except OSError as exc:
            return str(exc)
        if data == b"":
            return "peer closed"
        return "unexpected reply"
    except OSError as exc:
        return str(exc)
    finally:
        sock.close()


def render_has_readiness(render_url: str) -> str:
    sep = "&" if "?" in render_url else "?"
    query = urllib.parse.urlencode(
        [
            ("from", "-5min"),
            ("format", "json"),
            ("noCache", "1"),
            ("target", READINESS_METRIC),
        ]
    )
    try:
        with urllib.request.urlopen(f"{render_url}{sep}{query}", timeout=5) as resp:
            raw = resp.read().decode()
    except (urllib.error.URLError, TimeoutError) as exc:
        return str(exc)
    try:
        series_list = json.loads(raw)
    except json.JSONDecodeError as exc:
        return f"invalid json: {exc}"
    for series in series_list:
        for item in series.get("datapoints") or []:
            if item and item[0] is not None:
                return ""
    return f"readiness metric not rendered: {raw[:200]}"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("target", help="HOST:PORT")
    parser.add_argument(
        "--render-url",
        help="Optional Graphite /render URL to verify ingested readiness metric",
    )
    args = parser.parse_args()

    host, port_s = args.target.rsplit(":", 1)
    port = int(port_s)
    deadline = time.time() + 180
    last = "not checked"
    while time.time() < deadline:
        last = accept_line(host, port)
        if last:
            time.sleep(1)
            continue
        time.sleep(2)
        last = accept_line(host, port)
        if not last:
            break
        time.sleep(1)
    else:
        raise SystemExit(f"carbon {host}:{port} not ready: {last}")

    if not args.render_url:
        return

    next_send = 0.0
    last = "readiness metric not checked"
    while time.time() < deadline:
        now = time.time()
        if now >= next_send:
            err = accept_line(host, port)
            if err:
                last = err
                time.sleep(1)
                continue
            next_send = now + 2.0
        last = render_has_readiness(args.render_url)
        if not last:
            return
        time.sleep(0.5)
    raise SystemExit(f"graphite render {args.render_url} not ready: {last}")


if __name__ == "__main__":
    main()
