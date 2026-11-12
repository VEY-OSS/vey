#!/usr/bin/env python3
"""Console output of a site aggregate.

vey site metrics carry site, site_group, server, and stat_id. The collector
drops stat_id and server, so one emit is the sum for each site.
"""

import os
import socket
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import phase

LOG = os.environ["STATSD_LOG"]
REQUEST = "site.request.total"
ALIVE = "site.upstream.alive"


def send(lines):
    payload = "".join(f"{line}\n" for line in lines).encode()
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.sendto(payload, ("127.0.0.1", 8125))
    sock.close()


def parse_log():
    found = []
    for line in Path(LOG).read_text(errors="replace").splitlines():
        parts = line.split()
        if len(parts) < 3 or parts[1] not in (REQUEST, ALIVE):
            continue
        tags = {}
        for item in parts[3:]:
            if "=" not in item:
                continue
            key, value = item.split("=", 1)
            tags[key] = value
        found.append((parts[1], parts[2], tags))
    return found


def wait_until(predicate, timeout):
    deadline = time.time() + timeout
    found = []
    while time.time() < deadline:
        found = parse_log()
        if predicate(found):
            return found
        time.sleep(0.1)
    sys.stderr.write(Path(LOG).read_text(errors="replace")[-4000:])
    sys.exit(f"console output missing: {found}")


def site_rows(found, name):
    rows = []
    for metric, value, tags in found:
        if metric != name:
            continue
        if "stat_id" in tags or "server" in tags:
            sys.exit(f"joined tag still present: {tags}")
        rows.append((tags.get("site"), tags.get("site_group"), value))
    return rows


def main():
    phase.wait_listen()
    send(
        [
            "site.request.total:1|c|#site:shop,site_group:default,server:http,stat_id:11",
            "site.request.total:2|c|#site:shop,site_group:default,server:http,stat_id:12",
            "site.request.total:4|c|#site:shop,site_group:default,server:tls,stat_id:21",
            "site.request.total:8|c|#site:blog,site_group:default,server:http,stat_id:31",
            "site.upstream.alive:9|g|#site:shop,site_group:default,server:http,stat_id:11",
            "site.upstream.alive:1|g|#site:shop,site_group:default,server:tls,stat_id:21",
        ]
    )

    def first_window(found):
        return sorted(site_rows(found, REQUEST)) == [
            ("blog", "default", "8"),
            ("shop", "default", "7"),
        ] and ("shop", "default", "10") in site_rows(found, ALIVE)

    wait_until(first_window, 8)
    print("ok console aggregate by site")

    send(
        [
            "site.request.total:1|c|#site:shop,site_group:default,server:http,stat_id:11",
        ]
    )

    def next_window(found):
        return site_rows(found, REQUEST).count(("shop", "default", "1")) == 1

    wait_until(next_window, 8)
    print("ok console next window restarts the counter")


if __name__ == "__main__":
    main()
