# generate resource files
"${RUN_DIR}"/mkcert.sh

# start docker compose services (nginx, httpbin, ftp-server, influxdb, graphite)
[ -d /tmp/vsftpd ] || mkdir -p /tmp/vsftpd
docker compose -f "${PROJECT_DIR}"/scripts/coverage/vey-proxy/docker-compose.yml up -d
sleep 2

# start vey-dcgen
"${PROJECT_DIR}"/target/debug/vey-dcgen -c "${RUN_DIR}"/vey-dcgen.yaml -G port2999 &
DCGEN_PID=$!

# start vey-iploc
"${PROJECT_DIR}"/target/debug/vey-iploc -c "${RUN_DIR}"/vey-iploc.yaml -G port2888 &
IPLOC_PID=$!

# start vey-statsd
[ -n "${INFLUXDB3_AUTH_TOKEN}" ] || INFLUXDB3_AUTH_TOKEN=$(curl -X POST http://127.0.0.1:8181/api/v3/configure/token/admin | jq ".token" -r)
export INFLUXDB3_AUTH_TOKEN
"${PROJECT_DIR}"/target/debug/vey-statsd -c "${RUN_DIR}"/vey-statsd.yaml -G ${TEST_NAME} &
STATSD_PID=$!

# run vey-proxy integration tests

export TEST_CA_CERT_FILE="${RUN_DIR}/rootCA.pem"

vey_proxy_ctl()
{
	"${PROJECT_DIR}"/target/debug/vey-proxy-ctl -G ${TEST_NAME} -p $PROXY_PID "$@"
}

vey_proxy_ftp()
{
	"${PROJECT_DIR}"/target/debug/vey-proxy-ftp "$@"
}

set -x

"${PROJECT_DIR}"/target/debug/vey-proxy -Vvv

for dir in $(ls "${PROJECT_DIR}"/vey-proxy/examples)
do
	example_dir="${PROJECT_DIR}/vey-proxy/examples/${dir}"
	[ -d "${example_dir}" ] || continue

	"${PROJECT_DIR}"/target/debug/vey-proxy -c "${example_dir}" -t
done

for dir in $(find "${RUN_DIR}/" -type d | sort)
do
	[ -f "${dir}/main.yaml" ] || continue

	echo "=== ${dir}"
	date

	"${PROJECT_DIR}"/target/debug/vey-proxy -c "${dir}/main.yaml" -G ${TEST_NAME} &
	PROXY_PID=$!

	sleep 2

	[ -f "${dir}/testcases.sh" ] || continue
	TESTCASE_DIR=${dir}
	. "${dir}/testcases.sh"

	vey_proxy_ctl offline
	wait $PROXY_PID
done

set +x

kill -INT $STATSD_PID
kill -INT $IPLOC_PID
kill -INT $DCGEN_PID

## vey-proxy-ftp

echo "==== vey-proxy-ftp"
vey_proxy_ftp -u ftpuser -p ftppass 127.0.0.1 list
vey_proxy_ftp -u ftpuser -p ftppass 127.0.0.1 put --file "${RUN_DIR}/README.md" README
vey_proxy_ftp -u ftpuser -p ftppass 127.0.0.1 get README
vey_proxy_ftp -u ftpuser -p ftppass 127.0.0.1 del README

# cleanup

docker compose -f "${PROJECT_DIR}"/scripts/coverage/vey-proxy/docker-compose.yml down
