# Binaries default to a coverage build. PGO points BIN_DIR at a directory
# that contains the instrumented daemon and the control client.
BIN_DIR=${BIN_DIR:-${PROJECT_DIR}/target/debug}

UNIX_SOCK=/tmp/vey-dcgen-ci.sock

cleanup()
{
	if [ -n "${DCGEN_PID:-}" ]; then
		kill -INT "$DCGEN_PID" 2>/dev/null || true
		wait "$DCGEN_PID" 2>/dev/null || true
	fi
	rm -f "${UNIX_SOCK}" "${UNIX_SOCK}.client"
	if command -v clear_profiles >/dev/null 2>&1; then
		clear_profiles
	fi
	if [ -n "${PGO_BIN_DIR:-}" ]; then
		rm -rf "${PGO_BIN_DIR}"
	fi
	if [ -n "${PGO_RAW_DIR:-}" ]; then
		rm -rf "${PGO_RAW_DIR}"
	fi
}

trap cleanup EXIT

vey_dcgen_ctl()
{
	"${BIN_DIR}"/vey-dcgen-ctl -G ${TEST_NAME} -p $DCGEN_PID "$@"
}

query_udp()
{
	python3 "${RUN_DIR}/query.py" udp 127.0.0.1:2999 "$1"
}

query_unix()
{
	python3 "${RUN_DIR}/query.py" unix "${UNIX_SOCK}" "$1"
}

expect_datagram_miss()
{
	if "$@"; then
		echo "expected no datagram reply: $*" >&2
		exit 1
	fi
}

wait_until_ready()
{
	i=0
	while [ "$i" -lt 25 ]; do
		if ! kill -0 "$DCGEN_PID" 2>/dev/null; then
			echo "vey-dcgen exited before the control socket was ready" >&2
			wait "$DCGEN_PID" || true
			DCGEN_PID=
			exit 1
		fi
		if vey_dcgen_ctl version >/dev/null; then
			return
		fi
		i=$((i + 1))
		sleep 0.2
	done
	echo "timed out waiting for vey-dcgen" >&2
	exit 1
}

rm -f "${UNIX_SOCK}" "${UNIX_SOCK}.client"

set -x

"${BIN_DIR}"/vey-dcgen -Vvv

for dir in $(ls "${PROJECT_DIR}/vey-dcgen/examples")
do
	example_dir="${PROJECT_DIR}/vey-dcgen/examples/${dir}"
	[ -d "${example_dir}" ] || continue

	"${BIN_DIR}"/vey-dcgen -c "${example_dir}" -t
done

"${BIN_DIR}"/vey-dcgen -c "${RUN_DIR}" -t

"${BIN_DIR}"/vey-dcgen -c "${RUN_DIR}" -G ${TEST_NAME} &
DCGEN_PID=$!

wait_until_ready

query_udp www.example.com
query_unix www.example.com
expect_datagram_miss query_udp --invalid
expect_datagram_miss query_unix --invalid

vey_dcgen_ctl reload
query_udp www.example.com

# Let one stats emit interval pass before shutdown.
sleep 0.5

vey_dcgen_ctl offline
wait "$DCGEN_PID"
DCGEN_PID=

set +x
