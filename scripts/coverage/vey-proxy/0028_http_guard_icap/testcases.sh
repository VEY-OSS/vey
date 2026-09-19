#!/bin/sh

python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin.py" -T http://httpbin.local:8080 --no-auth
python3 "${PROJECT_DIR}/vey-proxy/ci/python3+requests/test_httpbin.py" -T http://httpbin.local:8080 --no-auth

for port in 8443 9443 9543
do
	python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin.py" \
		-T "https://httpbin.local:${port}" --no-auth --ca-cert "${TEST_CA_CERT_FILE}"
	python3 "${PROJECT_DIR}/vey-proxy/ci/python3+requests/test_httpbin.py" \
		-T "https://httpbin.local:${port}" --no-auth
done

if [ "${CURL_TEST_H2}" = "yes" ]
then
	for port in 10443 10543 11443 12443
	do
		python3 "${PROJECT_DIR}/vey-proxy/ci/python3+curl/test_httpbin_h2.py" --no-auth \
			-T "https://httpbin.local:${port}" --ca-cert "${TEST_CA_CERT_FILE}"
	done
fi
