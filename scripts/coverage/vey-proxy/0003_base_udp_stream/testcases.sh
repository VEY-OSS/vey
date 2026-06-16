#!/bin/sh

if [ "${CURL_TEST_H3}" = "yes" ]
then
	python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin_h3.py" -T https://httpbin.local:8443 --ca-cert "${TEST_CA_CERT_FILE}"
fi


python3 "${PROJECT_DIR}/vey-proxy/ci/python3+dns/test_dns.py" --dns-server 127.0.0.1 --dns-port 5353 --expected-ip 127.0.0.1 httpbin.local
python3 "${PROJECT_DIR}/vey-proxy/ci/python3+dns/test_dns.py" --dns-server 127.0.0.1 --dns-port 5353 --expected-ip 127.0.0.1 vey-proxy.local
