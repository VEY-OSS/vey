#!/usr/bin/env python3

"""Send one IP-locate datagram and wait for a reply.

The payload is a MessagePack fixstr of the IP, the same encoding as
Request::encode_new. Exit 0 when a reply arrives, 2 on timeout.
"""

import os
import socket
import sys


def encode_ip(ip: str) -> bytes:
    raw = ip.encode()
    if len(raw) > 31:
        raise SystemExit(f"ip {ip} is too long for a msgpack fixstr")
    return bytes([0xA0 | len(raw)]) + raw


def main() -> None:
    if len(sys.argv) != 4:
        raise SystemExit("usage: query.py udp|unix TARGET IP|--invalid")

    kind, target, ip = sys.argv[1:]
    payload = b"not a request" if ip == "--invalid" else encode_ip(ip)

    if kind == "udp":
        host, port = target.rsplit(":", 1)
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.settimeout(1.0)
        sock.sendto(payload, (host, int(port)))
    elif kind == "unix":
        client = target + ".client"
        try:
            os.unlink(client)
        except FileNotFoundError:
            pass
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
        sock.bind(client)
        sock.settimeout(1.0)
        try:
            sock.sendto(payload, target)
        except OSError:
            os.unlink(client)
            raise
    else:
        raise SystemExit(f"unknown transport {kind}")

    try:
        data, _addr = sock.recvfrom(4096)
    except (TimeoutError, socket.timeout):
        raise SystemExit(2)
    finally:
        sock.close()
        if kind == "unix":
            try:
                os.unlink(target + ".client")
            except FileNotFoundError:
                pass

    if not data:
        raise SystemExit("empty reply")


if __name__ == "__main__":
    main()
