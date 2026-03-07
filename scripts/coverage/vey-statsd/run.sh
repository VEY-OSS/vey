
# get influxdb auth token
[ -n "${INFLUXDB3_AUTH_TOKEN}" ] || INFLUXDB3_AUTH_TOKEN=$(curl -X POST http://127.0.0.1:8181/api/v3/configure/token/admin | jq ".token" -r)
export INFLUXDB3_AUTH_TOKEN

# run vey-statsd integration tests

vey_statsd_ctl()
{
	"${PROJECT_DIR}"/target/debug/vey-statsd-ctl -G ${TEST_NAME} -p $STATSD_PID "$@"
}

set -x

for dir in $(ls "${PROJECT_DIR}/vey-statsd/examples")
do
	example_dir="${PROJECT_DIR}/vey-statsd/examples/${dir}"
	[ -d "${example_dir}" ] || continue

	"${PROJECT_DIR}"/target/debug/vey-statsd -c "${example_dir}" -t
done

for dir in $(find "${RUN_DIR}/" -type d | sort)
do
	[ -f "${dir}/main.yaml" ] || continue

	echo "=== ${dir}"

	"${PROJECT_DIR}"/target/debug/vey-statsd -c "${dir}/main.yaml" -G ${TEST_NAME} &
	STATSD_PID=$!

	sleep 2

	[ -f "${dir}/testcases.sh" ] || continue
	TESTCASE_DIR=${dir}
	. "${dir}/testcases.sh"

	vey_statsd_ctl offline
	wait $STATSD_PID
done

set +x
