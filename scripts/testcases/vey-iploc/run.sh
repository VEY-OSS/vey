# Binaries default to a coverage build. PGO points BIN_DIR at a directory
# that contains the instrumented daemon and the control client.
BIN_DIR=${BIN_DIR:-${PROJECT_DIR}/target/debug}

UNIX_SOCK=/tmp/vey-iploc-ci.sock

cleanup()
{
	if [ -n "${IPLOC_PID:-}" ]; then
		kill -INT "$IPLOC_PID" 2>/dev/null || true
		wait "$IPLOC_PID" 2>/dev/null || true
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

vey_iploc_ctl()
{
	"${BIN_DIR}"/vey-iploc-ctl -G ${TEST_NAME} -p $IPLOC_PID "$@"
}

query_udp()
{
	python3 "${RUN_DIR}/query.py" udp 127.0.0.1:2888 "$1"
}

query_unix()
{
	python3 "${RUN_DIR}/query.py" unix "${UNIX_SOCK}" "$1"
}

expect_hit()
{
	out=$(vey_iploc_ctl query "$1")
	printf '%s\n' "$out" | grep -q "country: US"
	printf '%s\n' "$out" | grep -q "asn: $2"
}

expect_miss()
{
	if vey_iploc_ctl query "$1"; then
		echo "expected no location for $1" >&2
		exit 1
	fi
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
		if ! kill -0 "$IPLOC_PID" 2>/dev/null; then
			echo "vey-iploc exited before the control socket was ready" >&2
			wait "$IPLOC_PID" || true
			IPLOC_PID=
			exit 1
		fi
		if vey_iploc_ctl version >/dev/null; then
			return
		fi
		i=$((i + 1))
		sleep 0.2
	done
	echo "timed out waiting for vey-iploc" >&2
	exit 1
}

rm -f "${UNIX_SOCK}" "${UNIX_SOCK}.client"

set -x

"${BIN_DIR}"/vey-iploc -Vvv

for dir in $(ls "${PROJECT_DIR}/vey-iploc/examples")
do
	example_dir="${PROJECT_DIR}/vey-iploc/examples/${dir}"
	[ -d "${example_dir}" ] || continue

	"${BIN_DIR}"/vey-iploc -c "${example_dir}" -t
done

"${BIN_DIR}"/vey-iploc -c "${RUN_DIR}" -t

"${BIN_DIR}"/vey-iploc -c "${RUN_DIR}" -G ${TEST_NAME} &
IPLOC_PID=$!

wait_until_ready

expect_hit 1.1.1.1 13335
expect_hit 2001:db8::1 64500
expect_miss 203.0.113.1

query_udp 1.1.1.1
query_unix 2001:db8::1
expect_datagram_miss query_udp 203.0.113.1
expect_datagram_miss query_unix --invalid

vey_iploc_ctl reload
vey_iploc_ctl reload-country
vey_iploc_ctl reload-asn
expect_hit 1.1.1.1 13335

# Let one stats emit interval pass before shutdown.
sleep 0.5

vey_iploc_ctl offline
wait "$IPLOC_PID"
IPLOC_PID=

set +x
