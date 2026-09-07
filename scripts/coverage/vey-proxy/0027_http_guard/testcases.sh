#!/bin/sh

python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin.py" -T http://httpbin.local:8080 --no-auth
python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin.py" -T https://httpbin.local:8443 --no-auth --ca-cert "${TEST_CA_CERT_FILE}"
python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin.py" -T https://httpbin.local:9443 --no-auth --ca-cert "${TEST_CA_CERT_FILE}"

python3 "${PROJECT_DIR}/vey-proxy/ci/python3+requests/test_httpbin.py" -T http://httpbin.local:8080 --no-auth
python3 "${PROJECT_DIR}/vey-proxy/ci/python3+requests/test_httpbin.py" -T https://httpbin.local:8443 --no-auth
python3 "${PROJECT_DIR}/vey-proxy/ci/python3+requests/test_httpbin.py" -T https://httpbin.local:9443 --no-auth

# Sequential requests on one client connection so the site H1 pool can reuse origin.
curl -fsS -o /dev/null http://httpbin.local:8080/get http://httpbin.local:8080/headers
curl -fsS -o /dev/null --cacert "${TEST_CA_CERT_FILE}" \
	https://httpbin.local:9443/get https://httpbin.local:9443/headers

if [ "${CURL_TEST_H2}" = "yes" ]
then
	python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin_h2.py" \
		-T https://httpbin.local:10443 --ca-cert "${TEST_CA_CERT_FILE}"
	# Sequential requests so the site H2 pool can reuse the origin connection.
	curl -fsS --http2 -o /dev/null --cacert "${TEST_CA_CERT_FILE}" \
		https://httpbin.local:10443/get https://httpbin.local:10443/headers
fi
