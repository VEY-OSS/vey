#!/bin/sh

cleanup_origins()
{
	if [ -n "$ORIGIN_A_PID" ]
	then
		kill "$ORIGIN_A_PID" 2>/dev/null || true
		wait "$ORIGIN_A_PID" 2>/dev/null || true
	fi
	if [ -n "$ORIGIN_B_PID" ]
	then
		kill "$ORIGIN_B_PID" 2>/dev/null || true
		wait "$ORIGIN_B_PID" 2>/dev/null || true
	fi
}

expect_stdout()
{
	expected=$1
	shift
	got=$(mktemp)
	exp=$(mktemp)
	vey_proxy_ctl "$@" > "$got"
	printf '%s\n' "$expected" > "$exp"
	if ! cmp -s "$exp" "$got"
	then
		echo "unexpected output for: vey-proxy-ctl $*"
		echo "--- expected"
		cat "$exp"
		echo "--- got"
		cat "$got"
		rm -f "$got" "$exp"
		exit 1
	fi
	rm -f "$got" "$exp"
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

fetch_app()
{
	curl -sf --http1.0 --resolve "httpbin.local:8080:127.0.0.1" "http://httpbin.local:8080/"
}

expect_body()
{
	want=$1
	i=0
	while [ "$i" -lt 4 ]
	do
		got=$(fetch_app)
		if [ "$got" != "$want" ]
		then
			echo "expected upstream body ${want}, got ${got}"
			exit 1
		fi
		i=$((i + 1))
	done
}

set +e
(
	set -e
	ORIGIN_A_PID=
	ORIGIN_B_PID=
	trap cleanup_origins EXIT

	python3 "${TESTCASE_DIR}/origin.py" 18080 a &
	ORIGIN_A_PID=$!
	python3 "${TESTCASE_DIR}/origin.py" 18081 b &
	ORIGIN_B_PID=$!

	wait_origin 18080
	wait_origin 18081

	expect_stdout "$(printf 'addr\tconfig_weight\tweight\n127.0.0.1:18080\t1\t1\n127.0.0.1:18081\t2\t2')" \
		site-group weighted list-upstream app

	expect_stdout "notice: success" \
		site-group weighted set-upstream-weight app 127.0.0.1:18081 0
	expect_stdout "$(printf 'addr\tconfig_weight\tweight\n127.0.0.1:18080\t1\t1\n127.0.0.1:18081\t2\t0')" \
		site-group weighted list-upstream app
	expect_body a

	expect_stdout "notice: success" \
		site-group weighted set-upstream-weight app 127.0.0.1:18081 0.5
	expect_stdout "$(printf 'addr\tconfig_weight\tweight\n127.0.0.1:18080\t1\t1\n127.0.0.1:18081\t2\t0.5')" \
		site-group weighted list-upstream app

	expect_stdout "notice: success" \
		site-group weighted set-upstream-weight app 127.0.0.1:18080 0
	expect_body b

	expect_fail "no site group missing found" \
		site-group missing list-upstream app
	expect_fail "no site missing in site group weighted" \
		site-group weighted list-upstream missing
	expect_fail "site upstream has no peers defined" \
		site-group single list-upstream single
	expect_fail "site upstream has no peers defined" \
		site-group single set-upstream-weight single 127.0.0.1:80 1
	expect_fail "upstream address 127.0.0.1:9 is not configured" \
		site-group weighted set-upstream-weight app 127.0.0.1:9 1
	expect_fail "weight must be a finite number >= 0" \
		site-group weighted set-upstream-weight app 127.0.0.1:18080 inf
	expect_fail "weight must be a finite number >= 0" \
		site-group weighted set-upstream-weight app 127.0.0.1:18080 -- -1
	expect_fail "invalid upstream address" \
		site-group weighted set-upstream-weight app not-an-addr 1
)
code=$?
set -e
[ "$code" -eq 0 ]
