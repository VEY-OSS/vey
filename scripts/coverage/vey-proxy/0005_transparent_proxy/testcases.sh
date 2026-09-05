#!/bin/sh

python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin.py" -T http://httpbin.local:8080 --resolve httpbin.local:8080:[::1] --no-auth
python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin.py" -T https://httpbin.local:8443 --resolve httpbin.local:8443:[::1] --no-auth --ca-cert "${TEST_CA_CERT_FILE}"

# http_expose on 127.0.0.1:8080 with HTTPS origin; reuse via site H1 pool.
curl -fsS -o /dev/null --connect-to httpbin.local:8080:127.0.0.1:8080 \
	http://httpbin.local:8080/get http://httpbin.local:8080/headers
