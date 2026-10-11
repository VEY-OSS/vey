
vey_bench openssl httpbin.local:9443 --tls-ca-cert "${TEST_CA_CERT_FILE}"

vey_bench openssl 127.0.0.1:9443 --tls-name httpbin.local --tls-ca-cert "${TEST_CA_CERT_FILE}"

# JSON result

JSON_FILE="${JSON_OUT_DIR}/openssl.json"
vey_bench openssl httpbin.local:9443 --tls-ca-cert "${TEST_CA_CERT_FILE}" -n 3 -c 1 --json-file "${JSON_FILE}"
assert_json_report "${JSON_FILE}" openssl 1 3
assert_json_type "${JSON_FILE}" .connections object
assert_json_tcp_traffic "${JSON_FILE}"
assert_json_type "${JSON_FILE}" .tls.target object
assert_json_null "${JSON_FILE}" .histograms.conn_used_times
assert_json_null "${JSON_FILE}" .histograms.durations_ns.connect
