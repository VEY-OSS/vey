#!/bin/sh


test_socks5_proxy_http()
{
	date

	python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin.py" -x ${SOCKS5_PROXY} -T http://httpbin.local
	python3 "${PROJECT_DIR}/vey-proxy/ci/python3+requests/test_httpbin.py" -x ${SOCKS5_PROXY} -T http://httpbin.local
}


test_socks5_proxy_dns()
{
	date

	python3 "${PROJECT_DIR}/vey-proxy/ci/python3+dns/test_dns.py" -x ${SOCKS5_PROXY} --dns-server 127.0.0.53 --expected-ip 127.0.0.1 httpbin.local
	python3 "${PROJECT_DIR}/vey-proxy/ci/python3+dns/test_dns.py" -x ${SOCKS5_PROXY} --dns-server 127.0.0.53 --expected-ip 127.0.0.1 vey-proxy.local
}


test_socks4_proxy_http()
{
	date

	python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin.py" -x ${SOCKS4_PROXY} -T http://httpbin.local
}


SOCKS5_PROXY="socks5h://127.0.0.1:1080"
test_socks5_proxy_http
test_socks5_proxy_dns


SOCKS4_PROXY="socks4a://127.0.0.1:1080"
test_socks4_proxy_http


SOCKS5_PROXY="socks5h://127.0.0.1:1081"
test_socks5_proxy_http
test_socks5_proxy_dns


SOCKS4_PROXY="socks4a://127.0.0.1:1081"
test_socks4_proxy_http
