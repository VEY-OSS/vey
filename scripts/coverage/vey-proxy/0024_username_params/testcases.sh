#!/bin/sh

# HTTP proxy with proxy-basic auth username carrying params
HTTP_PROXY="http://user+label1=vey-proxy:toor@127.0.0.1:7080"
test_http_proxy_http_forward

HTTP_PROXY="http://user+label1=vey-proxy+session_id=abcd:toor@127.0.0.1:7080"
test_http_proxy_http_forward

# SOCKS5 proxy with username carrying params
SOCKS5_PROXY="socks5h://user+opt=vey-proxy:toor@127.0.0.1:1081"
test_socks5_proxy_http

SOCKS5_PROXY="socks5h://user+opt=vey-proxy+session_id=abcd:toor@127.0.0.1:1081"
test_socks5_proxy_http
