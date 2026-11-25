#!/bin/sh


HTTP_PROXY="http://127.0.0.1:8080"
test_http_proxy_http_forward
# stack escapers reject FTP control and transfer connections
#test_http_proxy_ftp_over_http
test_http_easy_proxy_https_forward
test_http_proxy_https_connect
test_http_proxy_https_forward


HTTPS_PROXY="https://vey-proxy.local:8443"
test_https_proxy_http_forward
#test_https_proxy_ftp_over_http
test_https_easy_proxy_https_forward
test_https_proxy_https_connect
test_https_proxy_https_forward


for port in 1080 1081
do
	SOCKS5_PROXY="socks5h://127.0.0.1:${port}"
	test_socks5_proxy_http
	test_socks5_proxy_https
	test_socks5_proxy_dns


	SOCKS4_PROXY="socks4a://127.0.0.1:${port}"
	test_socks4_proxy_http
	test_socks4_proxy_https
done
