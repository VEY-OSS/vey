
for port in 2443 8443
do

# GET

URL=https://httpbin.local:${port}/get

vey_bench h3 "${URL}" --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"

vey_bench h3 "${URL}" -H "Accept: application/json" --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"

vey_bench h3 "${URL}" -x socks5h://t1:toor@vey-proxy.local:1080 --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"

# POST

URL=https://httpbin.local:${port}/post

vey_bench h3 "${URL}" --method POST --payload 31323334 --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"
vey_bench h3 "${URL}" --method POST --payload 31323334 --binary --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"
vey_bench h3 "${URL}" --method POST --payload name=foo -H "Content-Type: application/x-www-form-urlencoded" --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"

done

# JSON result

JSON_FILE="${JSON_OUT_DIR}/h3.json"
vey_bench h3 https://httpbin.local:2443/get --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}" -n 3 -c 1 --json-file "${JSON_FILE}"
assert_json_report "${JSON_FILE}" h3 1 3
assert_json_type "${JSON_FILE}" .connections object
assert_json_udp_traffic "${JSON_FILE}"
assert_json_null "${JSON_FILE}" .tls
assert_json_http_histograms "${JSON_FILE}"
