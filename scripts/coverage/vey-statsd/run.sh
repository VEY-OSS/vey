cleanup_compose()
{
	docker compose -f "${PROJECT_DIR}"/scripts/coverage/vey-statsd/docker-compose.yml down
}

trap 'cleanup_compose; clear_profiles' EXIT

# start docker compose services (influxdb, graphite, prometheus, victoriametrics)
docker compose -f "${PROJECT_DIR}"/scripts/coverage/vey-statsd/docker-compose.yml up -d

# get influxdb auth token
wait4x -t 3m http http://127.0.0.1:8181
[ -n "${INFLUXDB3_AUTH_TOKEN}" ] || INFLUXDB3_AUTH_TOKEN=$(curl -X POST http://127.0.0.1:8181/api/v3/configure/token/admin | jq ".token" -r)
export INFLUXDB3_AUTH_TOKEN
export INFLUX_TOKEN="${INFLUXDB3_AUTH_TOKEN}"

wait4x -t 3m tcp 127.0.0.1:2003
wait4x -t 3m http http://127.0.0.1:9090/-/ready --expect-status-code 200
wait4x -t 3m http http://127.0.0.1:8428/health --expect-status-code 200
wait4x -t 3m tcp 127.0.0.1:4242

create_influx_db()
{
	code=$(curl -s -o /tmp/vey-statsd-influx-create.out -w '%{http_code}' \
		-X POST 'http://127.0.0.1:8181/api/v3/configure/database' \
		-H "Authorization: Bearer ${INFLUXDB3_AUTH_TOKEN}" \
		-H 'Content-Type: application/json' \
		-d "{\"db\":\"$1\"}")
	case "$code" in
		200|201|204|409) ;;
		*)
			echo "create influx database $1 failed: ${code}" >&2
			cat /tmp/vey-statsd-influx-create.out >&2
			exit 1
			;;
	esac
}

create_influx_db statsdv3
create_influx_db statsdv2


# run vey-statsd integration tests

vey_statsd_ctl()
{
	"${PROJECT_DIR}"/target/debug/vey-statsd-ctl -G ${TEST_NAME} -p $STATSD_PID "$@"
}

set -x

"${PROJECT_DIR}"/target/debug/vey-statsd -Vvv

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
	query_failed=
	. "${dir}/testcases.sh" || query_failed=1

	vey_statsd_ctl offline
	wait $STATSD_PID
	[ -z "$query_failed" ] || exit 1
done

set +x

cleanup_compose
