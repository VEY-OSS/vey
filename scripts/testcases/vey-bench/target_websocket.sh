
# Websocket H1

git clone https://github.com/gorilla/websocket.git --depth 1

cd websocket/examples/echo

go build server.go
./server -addr localhost:7080 &
WEBSOCKET_PID=$!

cd -

sleep 1

vey_bench websocket h1 ws://127.0.0.1:7080/echo --payload 1234512312 --check-message-length 10
vey_bench websocket h1 ws://127.0.0.1:7080/echo --payload 1234512312 --binary --check-message-length 5

## JSON result
JSON_FILE="${JSON_OUT_DIR}/websocket-h1.json"
vey_bench websocket h1 ws://127.0.0.1:7080/echo --payload 1234512312 --check-message-length 10 -n 3 -c 1 --json-file "${JSON_FILE}"
assert_json_report "${JSON_FILE}" websocket/h1 1 3
assert_json_type "${JSON_FILE}" .connections object
assert_json_tcp_traffic "${JSON_FILE}"
assert_hist_snapshot "${JSON_FILE}" .histograms.conn_used_times
assert_json_null "${JSON_FILE}" .histograms.durations_ns.connect

kill -INT $WEBSOCKET_PID
