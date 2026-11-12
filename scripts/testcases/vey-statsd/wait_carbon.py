#!/usr/bin/env python3
"""Wait until a Graphite carbon port stays open after a plaintext write.

Docker's published port accepts a TCP handshake before carbon is listening.
A write that is held open, twice, is the readiness check.
"""

import socket
import sys
import time


def accept_line(host: str, port: int) -> str:
    try:
        sock = socket.create_connection((host, port), timeout=2)
    except OSError as exc:
        return str(exc)
    try:
        sock.sendall(f"vey.readiness 1 {int(time.time())}\n".encode())
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


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: wait_carbon.py HOST:PORT")
    host, port_s = sys.argv[1].rsplit(":", 1)
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
            return
        time.sleep(1)
    raise SystemExit(f"carbon {host}:{port} not ready: {last}")


if __name__ == "__main__":
    main()
