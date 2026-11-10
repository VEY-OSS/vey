# Binaries default to a coverage build. PGO points BIN_DIR at a directory
# that contains the instrumented daemon, vey-iploc-ctl, and vey-iploc-get.
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

vey_iploc_get()
{
	"${BIN_DIR}"/vey-iploc-get "$@"
}

expect_get_hit()
{
	network=$1
	asn=$2
	shift 2
	out=$(vey_iploc_get "$@")
	printf '%s\n' "$out" | grep -q "network: ${network}"
	printf '%s\n' "$out" | grep -q "country: US"
	printf '%s\n' "$out" | grep -q "continent: NA"
	printf '%s\n' "$out" | grep -q "asn: ${asn}"
	printf '%s\n' "$out" | grep -q "ttl: 300"
}

expect_get_mixed()
{
	if out=$(vey_iploc_get "$@" 1.1.1.1 1.1.1.1 2001:db8::1 203.0.113.1); then
		echo "expected a missing address from vey-iploc-get $*" >&2
		exit 1
	fi
	printf '%s\n' "$out" | awk '
		function need(text) {
			if (index(rec, text) == 0) {
				printf "missing \"%s\" in record %d:\n%s\n", text, n, rec > "/dev/stderr"
				exit 1
			}
		}
		function hit(ip, network, asn) {
			need("ip: " ip)
			need("network: " network)
			need("country: US")
			need("continent: NA")
			need("asn: " asn)
			need("ttl: 300")
		}
		BEGIN { RS = "" }
		{
			n++
			rec = $0
			if (n == 1 || n == 2) hit("1.1.1.1", "1.1.1.0/24", "13335")
			else if (n == 3) hit("2001:db8::1", "2001:db8::/32", "64500")
			else if (n == 4) {
				need("ip: 203.0.113.1")
				need("not found")
			} else exit 1
		}
		END { if (n != 4) exit 1 }
	'
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

expect_get_hit 1.1.1.0/24 13335 1.1.1.1
expect_get_hit 2001:db8::/32 64500 --unix "${UNIX_SOCK}" 2001:db8::1
expect_get_mixed --udp 127.0.0.1:2888
expect_get_mixed --timeout 500ms --unix "${UNIX_SOCK}"

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
