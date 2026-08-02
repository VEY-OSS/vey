
# Http

test_http_get()
{
	URL=$1

  vey_bench h1 "${URL}" --ok-status 200
	vey_bench h1 "${URL}" -H "Accept: application/json" --ok-status 200

	vey_bench h1 "${URL}" -x http://t1:toor@vey-proxy.local:8080 --ok-status 200
	vey_bench h1 "${URL}" -x http://t1:toor@vey-proxy.local:8080 -p --ok-status 200

	vey_bench h1 "${URL}" -x https://t1:toor@vey-proxy.local:8443 --proxy-tls-ca-cert "${TEST_CA_CERT_FILE}" --ok-status 200
	vey_bench h1 "${URL}" -x https://t1:toor@vey-proxy.local:8443 --proxy-tls-ca-cert "${TEST_CA_CERT_FILE}" -p --ok-status 200

	vey_bench h1 "${URL}" -x socks5h://t1:toor@vey-proxy.local:1080 --ok-status 200
}

test_http_post()
{
	URL=$1

	vey_bench h1 "${URL}" --method POST --payload 31323334 --ok-status 200
	vey_bench h1 "${URL}" --method POST --payload 31323334 --binary --ok-status 200
	vey_bench h1 "${URL}" --method POST --payload name=foo -H "Content-Type: application/x-www-form-urlencoded" --ok-status 200
}

test_http_get http://httpbin.local/get
test_http_get http://httpbin.local:2080/get

test_http_post http://httpbin.local/post
test_http_post http://httpbin.local:2080/post

# Https

test_https_get()
{
	URL=$1

	vey_bench h1 "${URL}" --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"
	vey_bench h1 "${URL}" -H "Accept: application/json" --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"

	vey_bench h1 "${URL}" -x http://t1:toor@vey-proxy.local:8080 --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"
	vey_bench h1 "${URL}" -x http://t1:toor@vey-proxy.local:8080 -p --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"

	vey_bench h1 "${URL}" -x https://t1:toor@vey-proxy.local:8443 --proxy-tls-ca-cert "${TEST_CA_CERT_FILE}" --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"
	vey_bench h1 "${URL}" -x https://t1:toor@vey-proxy.local:8443 --proxy-tls-ca-cert "${TEST_CA_CERT_FILE}" -p --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"

	vey_bench h1 "${URL}" -x socks5h://t1:toor@vey-proxy.local:1080 --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"
}

test_https_post()
{
	URL=$1

	vey_bench h1 "${URL}" --method POST --payload 31323334 --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"
	vey_bench h1 "${URL}" --method POST --payload 31323334 --binary --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"
	vey_bench h1 "${URL}" --method POST --payload name=foo -H "Content-Type: application/x-www-form-urlencoded" --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"
}

test_https_get https://httpbin.local:9443/get
test_https_get https://httpbin.local:2443/get

test_https_post https://httpbin.local:9443/post
test_https_post https://httpbin.local:2443/post

# JSON result

JSON_FILE="${JSON_OUT_DIR}/h1-http.json"
vey_bench h1 http://httpbin.local/get --ok-status 200 -n 5 -c 2 --json-file "${JSON_FILE}"
assert_json_report "${JSON_FILE}" h1 2 5
assert_json_type "${JSON_FILE}" .global.requests_distribution object
assert_json_type "${JSON_FILE}" .connections object
assert_json_tcp_traffic "${JSON_FILE}"
assert_json_null "${JSON_FILE}" .tls
assert_json_http_histograms "${JSON_FILE}"

JSON_FILE="${JSON_OUT_DIR}/h1-https.json"
vey_bench h1 https://httpbin.local:9443/get --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}" -n 3 -c 1 --json-file "${JSON_FILE}"
assert_json_report "${JSON_FILE}" h1 1 3
assert_json_null "${JSON_FILE}" .global.requests_distribution
assert_json_tcp_traffic "${JSON_FILE}"
assert_json_type "${JSON_FILE}" .tls.target object
assert_json_gt "${JSON_FILE}" .tls.target.total 0
assert_json_null "${JSON_FILE}" .tls.proxy
assert_json_http_histograms "${JSON_FILE}"

JSON_FILE="${JSON_OUT_DIR}/h1-no-summary.json"
OUT=$(vey_bench h1 http://httpbin.local/get --ok-status 200 -n 2 --no-summary --json-file "${JSON_FILE}")
[ -z "${OUT}" ] || {
	echo "expected empty stdout with --no-summary, got: ${OUT}" >&2
	exit 1
}
assert_json_report "${JSON_FILE}" h1 1 2
