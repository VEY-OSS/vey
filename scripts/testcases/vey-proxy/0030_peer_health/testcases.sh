#!/bin/sh

DEAD=127.0.0.1:18092
LIVE=127.0.0.1:18093

cleanup()
{
	if [ -n "$ORIGIN_PID" ]
	then
		kill "$ORIGIN_PID" 2>/dev/null || true
		wait "$ORIGIN_PID" 2>/dev/null || true
	fi
	if [ -n "$DOMAIN_PID" ]
	then
		kill "$DOMAIN_PID" 2>/dev/null || true
		wait "$DOMAIN_PID" 2>/dev/null || true
	fi
}

expect_fail()
{
	pattern=$1
	shift
	set +e
	output=$(vey_proxy_ctl "$@" 2>&1)
	code=$?
	set -e
	if [ "$code" -eq 0 ]
	then
		echo "expected failure: vey-proxy-ctl $*"
		printf '%s\n' "$output"
		exit 1
	fi
	printf '%s\n' "$output" | grep -F -q "$pattern" || {
		echo "failure output did not match: ${pattern}"
		printf '%s\n' "$output"
		exit 1
	}
}

wait_origin()
{
	port=$1
	i=0
	while [ "$i" -lt 50 ]
	do
		if curl -sf "http://127.0.0.1:${port}/" >/dev/null
		then
			return
		fi
		sleep 0.1
		i=$((i + 1))
	done
	echo "origin ${port} did not start"
	exit 1
}

fetch_host()
{
	host=$1
	path=$2
	curl -s -o /tmp/vey-peer-health-body --http1.0 --max-time 5 \
		--resolve "${host}:8080:127.0.0.1" \
		-w '%{http_code}' \
		"http://${host}:8080${path}"
}

expect_status()
{
	status_host=$1
	status_path=$2
	status_want=$3
	status_got=$(fetch_host "$status_host" "$status_path")
	if [ "$status_got" != "$status_want" ]
	then
		echo "expected HTTP ${status_want} from ${status_host}${status_path}, got ${status_got}"
		exit 1
	fi
}

expect_body()
{
	body_host=$1
	body_want=$2
	expect_status "$body_host" / 200
	body_got=$(cat /tmp/vey-peer-health-body)
	if [ "$body_got" != "$body_want" ]
	then
		echo "expected upstream body ${body_want} from ${body_host}, got ${body_got}"
		exit 1
	fi
}

load_health()
{
	HEALTH=$(vey_proxy_ctl site-group health list-upstream-health app)
}

expect_peer()
{
	addr=$1
	fails=$2
	unavailable=$3
	recover=$4
	got_fails=$(printf '%s\n' "$HEALTH" | awk -F '\t' -v addr="$addr" '$1 == addr { print $2 }')
	got_unavailable=$(printf '%s\n' "$HEALTH" | awk -F '\t' -v addr="$addr" '$1 == addr { print $3 }')
	got_recover=$(printf '%s\n' "$HEALTH" | awk -F '\t' -v addr="$addr" '$1 == addr { print $4 }')
	if [ "$got_fails" != "$fails" ] || [ "$got_unavailable" != "$unavailable" ]
	then
		echo "unexpected health for ${addr}"
		printf '%s\n' "$HEALTH"
		exit 1
	fi
	case "$recover" in
		-)
			if [ "$got_recover" != "-" ]
			then
				echo "expected no recover time for ${addr}"
				printf '%s\n' "$HEALTH"
				exit 1
			fi
			;;
		+)
			case "$got_recover" in
				''|*[!0-9]*)
					echo "expected a recover time for ${addr}"
					printf '%s\n' "$HEALTH"
					exit 1
					;;
			esac
			if [ "$got_recover" -le 0 ]
			then
				echo "expected a positive recover time for ${addr}"
				printf '%s\n' "$HEALTH"
				exit 1
			fi
			;;
	esac
}

set +e
(
	set -e
	ORIGIN_PID=
	DOMAIN_PID=
	trap cleanup EXIT

	python3 "${PROJECT_DIR}/scripts/testcases/vey-proxy/0029_site_upstream/origin.py" 18093 b &
	ORIGIN_PID=$!
	python3 "${PROJECT_DIR}/scripts/testcases/vey-proxy/0029_site_upstream/origin.py" 18094 h &
	DOMAIN_PID=$!
	wait_origin 18093
	wait_origin 18094

	echo "==== weighted upstream starts healthy"
	load_health
	expect_peer "$DEAD" 0 false -
	expect_peer "$LIVE" 0 false -

	expect_status app.local / 521
	load_health
	expect_peer "$DEAD" 1 true +
	expect_peer "$LIVE" 0 false -

	expect_body app.local b
	expect_body app.local b
	load_health
	expect_peer "$DEAD" 1 true +
	expect_peer "$LIVE" 0 false -

	kill "$ORIGIN_PID"
	wait "$ORIGIN_PID" || true
	ORIGIN_PID=

	expect_status app.local / 521
	load_health
	expect_peer "$DEAD" 1 true +
	expect_peer "$LIVE" 1 true +

	expect_status app.local / 521
	load_health
	expect_peer "$DEAD" 2 true +
	expect_peer "$LIVE" 1 true +

	expect_fail "peer health check is not enabled" \
		site-group health list-upstream-health plain
	expect_fail "site upstream has no peers defined" \
		site-group health list-upstream-health single

	expect_status closed.local / 521
	expect_status closed.local / 521
	expect_body httpbin.local h
)
code=$?
set -e
rm -f /tmp/vey-peer-health-body
[ "$code" -eq 0 ]
