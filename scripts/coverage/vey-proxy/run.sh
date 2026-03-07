# generate resource files
"${RUN_DIR}"/mkcert.sh

# start nginx
[ -d /tmp/nginx ] || mkdir /tmp/nginx
/usr/sbin/nginx -c "${PROJECT_DIR}"/scripts/coverage/vey-proxy/nginx.conf

# start glauth
git clone https://github.com/glauth/glauth --depth 1
cd glauth/v2
cp "${PROJECT_DIR}"/scripts/coverage/vey-proxy/*pem .
go build
./glauth -c "${PROJECT_DIR}"/scripts/coverage/vey-proxy/glauth.cfg &
GLAUTH_PID=$!
cd -

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
kill -INT $GLAUTH_PID
NGINX_PID=$(cat /tmp/nginx.pid)
kill -INT $NGINX_PID

## vey-proxy-ftp

echo "==== vey-proxy-ftp"
vey_proxy_ftp -u ftpuser -p ftppass 127.0.0.1 list
vey_proxy_ftp -u ftpuser -p ftppass 127.0.0.1 put --file "${RUN_DIR}/README.md" README
vey_proxy_ftp -u ftpuser -p ftppass 127.0.0.1 get README
vey_proxy_ftp -u ftpuser -p ftppass 127.0.0.1 del README
