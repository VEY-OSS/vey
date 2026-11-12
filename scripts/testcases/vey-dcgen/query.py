#!/usr/bin/env python3

"""Send one certificate-request datagram and wait for a reply.

A bare host is a MessagePack fixstr. ``--site`` and ``--usage`` send a map.
Exit 0 when the reply matches the requested checks, 2 on timeout.
"""

import argparse
import os
import socket
import sys


def encode_str(text: str) -> bytes:
    raw = text.encode()
    n = len(raw)
    if n <= 31:
        return bytes([0xA0 | n]) + raw
    if n <= 255:
        return bytes([0xD9, n]) + raw
    if n <= 65535:
        return bytes([0xDA, n >> 8, n & 0xFF]) + raw
    raise SystemExit(f"string is too long to encode: {text[:32]}")


def encode_uint(value: int) -> bytes:
    if value < 0:
        raise SystemExit(f"negative integer {value}")
    if value <= 0x7F:
        return bytes([value])
    if value <= 0xFF:
        return bytes([0xCC, value])
    if value <= 0xFFFF:
        return bytes([0xCD, value >> 8, value & 0xFF])
    if value <= 0xFFFFFFFF:
        return bytes(
            [0xCE, (value >> 24) & 0xFF, (value >> 16) & 0xFF, (value >> 8) & 0xFF, value & 0xFF]
        )
    raise SystemExit(f"integer is too large to encode: {value}")


def encode_request(host: str, site: str, usage: int | None) -> bytes:
    if host == "--invalid":
        return b"not a request"
    if not site and usage is None:
        return encode_str(host)
    pairs = [(1, encode_str(host))]
    if usage is not None:
        pairs.append((4, encode_uint(usage)))
    if site:
        pairs.append((5, encode_str(site)))
    if len(pairs) > 15:
        raise SystemExit("request map is too large")
    buf = bytearray([0x80 | len(pairs)])
    for key, value in pairs:
        buf += encode_uint(key)
        buf += value
    return bytes(buf)


class MsgpackReader:
    def __init__(self, data: bytes) -> None:
        self.data = data
        self.i = 0

    def take(self, n: int) -> bytes:
        end = self.i + n
        if end > len(self.data):
            raise SystemExit("truncated msgpack reply")
        chunk = self.data[self.i:end]
        self.i = end
        return chunk

    def u8(self) -> int:
        return self.take(1)[0]

    def read(self):
        tag = self.u8()
        if tag <= 0x7F:
            return tag
        if tag >= 0xE0:
            return tag - 256
        if 0xA0 <= tag <= 0xBF:
            return self.take(tag & 0x1F).decode()
        if 0x80 <= tag <= 0x8F:
            return self.read_map(tag & 0x0F)
        if tag == 0xDE:
            n = int.from_bytes(self.take(2), "big")
            return self.read_map(n)
        if tag == 0xDF:
            n = int.from_bytes(self.take(4), "big")
            return self.read_map(n)
        if tag == 0xD9:
            return self.take(self.u8()).decode()
        if tag == 0xDA:
            return self.take(int.from_bytes(self.take(2), "big")).decode()
        if tag == 0xDB:
            return self.take(int.from_bytes(self.take(4), "big")).decode()
        if tag == 0xC4:
            return self.take(self.u8())
        if tag == 0xC5:
            return self.take(int.from_bytes(self.take(2), "big"))
        if tag == 0xC6:
            return self.take(int.from_bytes(self.take(4), "big"))
        if tag == 0xCC:
            return self.u8()
        if tag == 0xCD:
            return int.from_bytes(self.take(2), "big")
        if tag == 0xCE:
            return int.from_bytes(self.take(4), "big")
        if tag == 0xCF:
            return int.from_bytes(self.take(8), "big")
        if tag == 0xD0:
            return int.from_bytes(self.take(1), "big", signed=True)
        if tag == 0xD1:
            return int.from_bytes(self.take(2), "big", signed=True)
        if tag == 0xD2:
            return int.from_bytes(self.take(4), "big", signed=True)
        if tag == 0xD3:
            return int.from_bytes(self.take(8), "big", signed=True)
        raise SystemExit(f"unsupported msgpack tag {tag:#x}")

    def read_map(self, n: int) -> dict:
        out = {}
        for _ in range(n):
            key = self.read()
            out[key] = self.read()
        return out


def pem_body(text: str) -> str:
    lines = []
    inside = False
    for line in text.splitlines():
        if "BEGIN CERTIFICATE" in line:
            if inside:
                break
            inside = True
            continue
        if "END CERTIFICATE" in line:
            break
        if inside:
            lines.append(line.strip())
    if not lines:
        raise SystemExit("certificate PEM has no body")
    return "".join(lines)


def first_cert_body(pem: str) -> str:
    return pem_body(pem)


def main() -> None:
    if len(sys.argv) == 4 and sys.argv[3] == "--invalid":
        kind, target, host = sys.argv[1:]
        args = argparse.Namespace(
            kind=kind,
            target=target,
            host=host,
            site="",
            usage=None,
            expect_cert=None,
            expect_ttl=None,
            expect_generated=False,
            reject_cert=[],
        )
    else:
        parser = argparse.ArgumentParser(description="query one vey-dcgen certificate")
        parser.add_argument("kind", choices=("udp", "unix"))
        parser.add_argument("target")
        parser.add_argument("host")
        parser.add_argument("--site", default="")
        parser.add_argument("--usage", type=int)
        parser.add_argument("--expect-cert")
        parser.add_argument("--expect-ttl", type=int)
        parser.add_argument("--expect-generated", action="store_true")
        parser.add_argument("--reject-cert", action="append", default=[])
        args = parser.parse_args()

    payload = encode_request(args.host, args.site, args.usage)
    kind = args.kind
    target = args.target

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

    reply = MsgpackReader(data).read()
    if not isinstance(reply, dict):
        raise SystemExit("reply is not a msgpack map")
    cert = reply.get(3)
    ttl = reply.get(5)
    site = reply.get(7, "")
    if not isinstance(cert, str) or "BEGIN CERTIFICATE" not in cert:
        raise SystemExit("reply certificate field is missing")
    if not isinstance(ttl, int):
        raise SystemExit(f"reply ttl is missing: {ttl!r}")
    if not isinstance(site, str):
        raise SystemExit(f"reply site is not a string: {site!r}")

    if args.site:
        if site != args.site:
            raise SystemExit(f"reply site {site!r} != {args.site!r}")
    elif site:
        raise SystemExit(f"reply included unexpected site {site!r}")

    got = first_cert_body(cert)
    if args.expect_cert:
        want = first_cert_body(open(args.expect_cert, encoding="ascii").read())
        if got != want:
            raise SystemExit(f"reply certificate does not match {args.expect_cert}")
    if args.expect_ttl is not None and ttl != args.expect_ttl:
        raise SystemExit(f"reply ttl {ttl} != {args.expect_ttl}")
    for path in args.reject_cert:
        rejected = first_cert_body(open(path, encoding="ascii").read())
        if got == rejected:
            raise SystemExit(f"reply certificate unexpectedly matches {path}")
    if args.expect_generated and ttl < 300:
        raise SystemExit(f"generated certificate ttl {ttl} is below the issuer minimum")

    print(f"ttl: {ttl}")
    if site:
        print(f"site: {site}")


if __name__ == "__main__":
    main()
