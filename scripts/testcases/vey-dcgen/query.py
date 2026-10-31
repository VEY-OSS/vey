#!/usr/bin/env python3

"""Send one certificate-request datagram and wait for a reply.

The payload is a MessagePack fixstr of the hostname. Exit 0 when a reply
contains a PEM certificate, 2 on timeout.
"""

import os
import socket
import sys


def encode_host(host: str) -> bytes:
    raw = host.encode()
    if len(raw) > 31:
        raise SystemExit(f"host {host} is too long for a msgpack fixstr")
    return bytes([0xA0 | len(raw)]) + raw


def main() -> None:
    if len(sys.argv) != 4:
        raise SystemExit("usage: query.py udp|unix TARGET HOST|--invalid")

    kind, target, host = sys.argv[1:]
    payload = b"not a request" if host == "--invalid" else encode_host(host)

    if kind == "udp":
        host_addr, port = target.rsplit(":", 1)
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.settimeout(2.0)
        sock.sendto(payload, (host_addr, int(port)))
    elif kind == "unix":
        client = target + ".client"
        try:
            os.unlink(client)
        except FileNotFoundError:
            pass
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
        sock.bind(client)
        sock.settimeout(2.0)
        try:
            sock.sendto(payload, target)
        except OSError:
            os.unlink(client)
            raise
    else:
        raise SystemExit(f"unknown transport {kind}")

    try:
        data, _addr = sock.recvfrom(65535)
    except (TimeoutError, socket.timeout):
        raise SystemExit(2)
    finally:
        sock.close()
        if kind == "unix":
            try:
                os.unlink(target + ".client")
            except FileNotFoundError:
                pass

    if b"BEGIN CERTIFICATE" not in data:
        raise SystemExit("reply did not contain a certificate")


if __name__ == "__main__":
    main()
