#!/usr/bin/env python3
"""Console output after regulate.

regulate adds a name prefix and drops tags. It does not sum samples, so two
counters that share a site stay two lines.
"""

import os
import socket
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import phase

LOG = os.environ["STATSD_LOG"]
REQUEST = "vey.site.request.total"


def send(lines):
    payload = "".join(f"{line}\n" for line in lines).encode()
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.sendto(payload, ("127.0.0.1", 8125))
    sock.close()


def parse_log():
    found = []
    for line in Path(LOG).read_text(errors="replace").splitlines():
        parts = line.split()
        if len(parts) < 3 or parts[1] != REQUEST:
            continue
        tags = {}
        for item in parts[3:]:
            if "=" not in item:
                continue
            key, value = item.split("=", 1)
            tags[key] = value
        if "stat_id" in tags:
            sys.exit(f"dropped tag still present: {tags}")
        found.append((parts[2], tags))
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


def main():
    phase.wait_listen()
    send(
        [
            "site.request.total:1|c|#site:shop,site_group:default,server:http,stat_id:11",
            "site.request.total:2|c|#site:shop,site_group:default,server:http,stat_id:12",
            "site.request.total:5|c|#site:blog,stat_id:9",
        ]
    )

    def rewritten(found):
        return sorted((value, tuple(sorted(tags.items()))) for value, tags in found) == [
            ("1", (("server", "http"), ("site", "shop"), ("site_group", "default"))),
            ("2", (("server", "http"), ("site", "shop"), ("site_group", "default"))),
            ("5", (("site", "blog"),)),
        ]

    wait_until(rewritten, 8)
    print("ok console regulate prefix and drop_tags")


if __name__ == "__main__":
    main()
