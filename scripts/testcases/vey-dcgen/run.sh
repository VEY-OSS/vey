# Binaries default to a coverage build. PGO points BIN_DIR at a directory
# that contains the instrumented daemon, vey-dcgen-ctl, and vey-dcgen-get.
BIN_DIR=${BIN_DIR:-${PROJECT_DIR}/target/debug}

UNIX_SOCK=/tmp/vey-dcgen-ci.sock
SITE_DIR="${RUN_DIR}/site"
SITE_YAML="${SITE_DIR}/shop.yaml"
SITE_YAML_BACKUP=

cleanup()
{
	if [ -n "${DCGEN_PID:-}" ]; then
		kill -INT "$DCGEN_PID" 2>/dev/null || true
		wait "$DCGEN_PID" 2>/dev/null || true
	fi
	if [ -n "${SITE_YAML_BACKUP}" ] && [ -f "${SITE_YAML_BACKUP}" ]; then
		cp "${SITE_YAML_BACKUP}" "${SITE_YAML}"
		rm -f "${SITE_YAML_BACKUP}"
	fi
	rm -f "${SITE_DIR}/shop.crt" "${SITE_DIR}/shop.key" \
		"${SITE_DIR}/sign.crt" "${SITE_DIR}/sign.key"
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

vey_dcgen_get()
{
	"${BIN_DIR}"/vey-dcgen-get "$@"
}

expect_get_cert()
{
	host=$1
	shift
	out=$(vey_dcgen_get "$@" "$host")
	printf '%s\n' "$out" | grep -q "^ttl: "
	printf '%s\n' "$out" | grep -q "BEGIN CERTIFICATE"
	printf '%s\n' "$out" | grep -q "BEGIN PRIVATE KEY"
}

query_udp()
{
	python3 "${RUN_DIR}/query.py" udp 127.0.0.1:2999 "$1"
}

query_unix()
{
	python3 "${RUN_DIR}/query.py" unix "${UNIX_SOCK}" "$1"
}

query_site()
{
	kind=$1
	shift
	if [ "$kind" = "unix" ]; then
		python3 "${RUN_DIR}/query.py" unix "${UNIX_SOCK}" "$@"
	else
		python3 "${RUN_DIR}/query.py" udp 127.0.0.1:2999 "$@"
	fi
}

expect_reload_site_fail()
{
	if vey_dcgen_ctl reload-site "$1"; then
		echo "reload-site $1 should fail" >&2
		exit 1
	fi
}

write_site_yaml()
{
	cat > "${SITE_YAML}" <<EOF
id: $1
ttl: $2
certificate: shop.crt
private_key: shop.key
sign_certificate: sign.crt
sign_private_key: sign.key
EOF
}

prepare_site_certs()
{
	SITE_YAML_BACKUP=$(mktemp)
	cp "${SITE_YAML}" "${SITE_YAML_BACKUP}"
	openssl ecparam -name prime256v1 -genkey -noout -out "${SITE_DIR}/shop.key"
	openssl req -new -x509 -key "${SITE_DIR}/shop.key" -out "${SITE_DIR}/shop.crt" \
		-days 30 -subj "/CN=predefined-shop"
	openssl ecparam -name prime256v1 -genkey -noout -out "${SITE_DIR}/sign.key"
	openssl req -new -x509 -key "${SITE_DIR}/sign.key" -out "${SITE_DIR}/sign.crt" \
		-days 30 -subj "/CN=predefined-sign"
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
prepare_site_certs

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

expect_get_cert www.example.com
expect_get_cert www.example.com --unix "${UNIX_SOCK}"

# Predefined site certificates. Usage 0 and 1 share the TLS pair. Usage 11
# is the TLCP signature pair. A missing pair or site is signed by the issuer.
query_site udp www.example.com --site shop --usage 0 \
	--expect-cert "${SITE_DIR}/shop.crt" --expect-ttl 120
query_site unix www.example.com --site shop --usage 1 \
	--expect-cert "${SITE_DIR}/shop.crt" --expect-ttl 120
query_site udp www.example.net --site shop --usage 11 \
	--expect-cert "${SITE_DIR}/sign.crt" --expect-ttl 120
query_site udp www.example.org --site shop --usage 12 \
	--expect-generated \
	--reject-cert "${SITE_DIR}/shop.crt" \
	--reject-cert "${SITE_DIR}/sign.crt"
query_site udp www.example.com --site other --usage 0 \
	--expect-generated \
	--reject-cert "${SITE_DIR}/shop.crt" \
	--reject-cert "${SITE_DIR}/sign.crt"

expect_reload_site_fail nosuch
write_site_yaml other 45
expect_reload_site_fail shop
query_site udp www.example.com --site shop --usage 0 \
	--expect-cert "${SITE_DIR}/shop.crt" --expect-ttl 120

write_site_yaml shop 45
vey_dcgen_ctl reload-site shop
query_site udp www.example.com --site shop --usage 0 \
	--expect-cert "${SITE_DIR}/shop.crt" --expect-ttl 45
query_site unix www.example.com --site shop --usage 11 \
	--expect-cert "${SITE_DIR}/sign.crt" --expect-ttl 45

vey_dcgen_ctl reload
query_udp www.example.com
query_site udp www.example.com --site shop --usage 0 \
	--expect-cert "${SITE_DIR}/shop.crt" --expect-ttl 45

vey_dcgen_ctl reload-issuer
query_udp www.example.com
query_site udp www.example.com --site shop --usage 0 \
	--expect-cert "${SITE_DIR}/shop.crt" --expect-ttl 45

cp "${SITE_YAML_BACKUP}" "${SITE_YAML}"
vey_dcgen_ctl reload
query_site udp www.example.com --site shop --usage 0 \
	--expect-cert "${SITE_DIR}/shop.crt" --expect-ttl 120

# Let one stats emit interval pass before shutdown.
sleep 0.5

vey_dcgen_ctl offline
wait "$DCGEN_PID"
DCGEN_PID=

set +x
