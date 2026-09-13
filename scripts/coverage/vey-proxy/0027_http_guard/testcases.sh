#!/bin/sh

python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin.py" -T http://httpbin.local:8080 --no-auth
python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin.py" -T https://httpbin.local:8443 --no-auth --ca-cert "${TEST_CA_CERT_FILE}"
python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin.py" -T https://httpbin.local:9443 --no-auth --ca-cert "${TEST_CA_CERT_FILE}"

python3 "${PROJECT_DIR}/vey-proxy/ci/python3+requests/test_httpbin.py" -T http://httpbin.local:8080 --no-auth
python3 "${PROJECT_DIR}/vey-proxy/ci/python3+requests/test_httpbin.py" -T https://httpbin.local:8443 --no-auth
python3 "${PROJECT_DIR}/vey-proxy/ci/python3+requests/test_httpbin.py" -T https://httpbin.local:9443 --no-auth

if [ "${CURL_TEST_H2}" = "yes" ]
then
	python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin_h2.py" --no-auth \
		-T https://httpbin.local:10443 --ca-cert "${TEST_CA_CERT_FILE}"
fi
