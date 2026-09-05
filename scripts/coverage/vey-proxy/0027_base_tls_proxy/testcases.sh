#!/bin/sh

python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin.py" -T https://httpbin.local:8443 --ca-cert "${TEST_CA_CERT_FILE}"
python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin.py" -T https://vey-proxy.local:8443 --ca-cert "${TEST_CA_CERT_FILE}"
python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin.py" -T https://httpbin.local:9443 --ca-cert "${TEST_CA_CERT_FILE}"

python3 "${PROJECT_DIR}/vey-proxy/ci/python3+requests/test_httpbin.py" -T https://httpbin.local:8443 --ca-cert "${TEST_CA_CERT_FILE}"
python3 "${PROJECT_DIR}/vey-proxy/ci/python3+requests/test_httpbin.py" -T https://httpbin.local:9443 --ca-cert "${TEST_CA_CERT_FILE}"
