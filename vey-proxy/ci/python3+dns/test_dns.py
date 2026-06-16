#!/usr/bin/env python3

import argparse
import time
import sys
import socket
import unittest
from urllib.parse import urlparse

import socks
import dns.message
import dns.query

proxy_url = None
dns_server = '127.0.0.53'
dns_port = 53
domain = "one.one.one.one"
expected_ip = None
rdata_type = dns.rdatatype.A


class TestDns(unittest.TestCase):
    def setUp(self):
        if proxy_url is not None:
            url = urlparse(proxy_url)
            family = dns.inet.af_for_address(str(url.hostname))
            self.sock = socks.socksocket(family, type=socket.SOCK_DGRAM, proto=0)
            self.sock.set_proxy(proxy_type=socks.SOCKS5, addr=url.hostname, port=url.port,
                                username=url.username, password=url.password)
        else:
            family = dns.inet.af_for_address(dns_server)
            self.sock = socket.socket(family, socket.SOCK_DGRAM)

    def tearDown(self):
        self.sock.close()

    def test_query(self):
        msg = dns.message.make_query(domain, rdata_type, rdclass=dns.rdataclass.IN, payload=4096)
        msg.flags |= dns.flags.AD
        msg.flags |= dns.flags.RD
        dns.query.send_udp(self.sock, msg, (dns_server, dns_port))
        (rsp, _) = dns.query.receive_udp(self.sock, (dns_server, dns_port),
                                         expiration=time.time() + 30)
        print(rsp)
        ips = []
        for rr_set in rsp.answer:
            if rr_set.rdtype != rdata_type:
                continue
            for rr in rr_set.to_rdataset():
                ips.append(rr.to_text())

        self.assertGreaterEqual(len(ips), 1)
        if expected_ip is not None:
            self.assertIn(expected_ip, ips)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("-x", "--proxy", nargs='?', help="Proxy URL")
    parser.add_argument("--dns-server", nargs='?', help="DNS server IP", default="127.0.0.53")
    parser.add_argument("--dns-port", nargs='?', type=int, help="DNS server Port", default="53")
    parser.add_argument("--rdata-type", nargs='?', help="The query RDATA Type", default='A')
    parser.add_argument("--expected-ip", nargs='?', help="Expected IP Address")
    parser.add_argument("domain", nargs='?')

    (args, left_args) = parser.parse_known_args()

    if args.proxy is not None:
        proxy_url = args.proxy
    if args.domain is not None:
        domain = args.domain
    if args.rdata_type is not None:
        rdata_type = dns.rdatatype.from_text(args.rdata_type)
    if args.expected_ip is not None:
        expected_ip = args.expected_ip
    dns_server = args.dns_server
    dns_port = args.dns_port

    left_args.insert(0, sys.argv[0])

    unittest.main(argv=left_args)
