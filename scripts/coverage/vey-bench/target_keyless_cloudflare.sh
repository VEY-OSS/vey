
# Start a local keyless server for the cloudflare protocol target
# Local store only loads files with a ".key" extension.
KEYS_DIR="${RUN_DIR}/keyless/keys"
mkdir -p "${KEYS_DIR}"
cp "${TEST_EC_KEY_FILE}" "${KEYS_DIR}/ec.key"

"${PROJECT_DIR}"/target/debug/vey-keyless -c "${RUN_DIR}/keyless/main.yaml" -G "${TEST_NAME}-keyless" &
KEYLESS_PID=$!
sleep 2

TARGET_PARAMS="keyless cloudflare --no-tls --target 127.0.0.1:1300 --key ${TEST_EC_KEY_FILE} --sign --digest-type sha256 --verify 4d4dfb668f8c6ddd0227c03907515c58779914098a1bf8c169faafdea4d1b91d"

vey_bench ${TARGET_PARAMS}
vey_bench -c 2 -n 4 ${TARGET_PARAMS}
vey_bench -c 1 -t 2 --emit-metrics ${TARGET_PARAMS}

# JSON result
JSON_FILE="${JSON_OUT_DIR}/keyless-cloudflare.json"
vey_bench -c 2 -n 4 --json-file "${JSON_FILE}" ${TARGET_PARAMS}
assert_json_report "${JSON_FILE}" keyless/cloudflare 2 4
assert_json_type "${JSON_FILE}" .connections object
assert_json_tcp_traffic "${JSON_FILE}"
assert_json_null "${JSON_FILE}" .tls
assert_hist_snapshot "${JSON_FILE}" .histograms.conn_used_times
assert_json_null "${JSON_FILE}" .histograms.durations_ns.connect

"${PROJECT_DIR}"/target/debug/vey-keyless-ctl -G "${TEST_NAME}-keyless" -p $KEYLESS_PID offline
wait $KEYLESS_PID
rm -rf "${KEYS_DIR}"
