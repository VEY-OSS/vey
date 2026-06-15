#!/bin/sh

python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin_h3.py" -T https://httpbin.local:8443 --ca-cert "${TEST_CA_CERT_FILE}"
