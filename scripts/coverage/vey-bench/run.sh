# generate resource files
"${RUN_DIR}"/mkcert.sh

# start vey-proxy
"${PROJECT_DIR}"/target/debug/vey-proxy -c "${RUN_DIR}"/vey-proxy.yaml -G "${TEST_NAME}" &
PROXY_PID=$!

# start nginx
[ -d /tmp/nginx ] || mkdir /tmp/nginx
/usr/sbin/nginx -c "${PROJECT_DIR}"/scripts/coverage/vey-bench/nginx.conf

# start vey-statsd
[ -n "${INFLUX_TOKEN}" ] || INFLUX_TOKEN=$(curl -X POST http://127.0.0.1:8181/api/v3/configure/token/admin | jq ".token" -r)
export INFLUX_TOKEN
"${PROJECT_DIR}"/target/debug/vey-statsd -c "${RUN_DIR}"/vey-statsd.yaml -G ${TEST_NAME} &
STATSD_PID=$!

# run vey-bench integration tests

export TEST_CA_CERT_FILE="${RUN_DIR}/rootCA.pem"
export TEST_RSA_KEY_FILE="${RUN_DIR}/rootCA-RSA-key.pem"
export TEST_RSA_CERT_FILE="${RUN_DIR}/rootCA-RSA.pem"
export TEST_EC_KEY_FILE="${RUN_DIR}/rootCA-EC-key.pem"

vey_bench()
{
	"${PROJECT_DIR}"/target/debug/vey-bench --no-progress-bar --log-error 1 "$@"
}

set -x

. "${RUN_DIR}"/target_dns.sh
. "${RUN_DIR}"/target_h1.sh
. "${RUN_DIR}"/target_h2.sh
. "${RUN_DIR}"/target_keyless_openssl.sh
. "${RUN_DIR}"/target_openssl.sh
. "${RUN_DIR}"/target_rustls.sh
. "${RUN_DIR}"/target_thrift_tcp.sh
. "${RUN_DIR}"/target_websocket.sh

set +x

"${PROJECT_DIR}"/target/debug/vey-proxy-ctl -G "${TEST_NAME}" -p $PROXY_PID offline

kill -INT $STATSD_PID
NGINX_PID=$(cat /tmp/nginx.pid)
kill -INT $NGINX_PID
