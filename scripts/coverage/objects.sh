# Cargo 1.100 stores test binaries outside target/debug/deps.
# Repeat the cargo invocation with --message-format=json and read the
# executable paths from compiler-artifact messages.

coverage_test_objects() {
	"$@"
	_coverage_json=$(mktemp)
	_coverage_bin=$1
	_coverage_cmd=$2
	shift 2
	if [ "${_coverage_cmd}" = "test" ]; then
		"${_coverage_bin}" "${_coverage_cmd}" --no-run --message-format=json "$@" >"${_coverage_json}"
	else
		"${_coverage_bin}" "${_coverage_cmd}" --message-format=json "$@" >"${_coverage_json}"
	fi
	set +e
	all_objects=$(python3 "${PROJECT_DIR}/scripts/coverage/objects.py" ${all_binaries} < "${_coverage_json}")
	_coverage_status=$?
	set -e
	rm -f "${_coverage_json}"
	[ "${_coverage_status}" -eq 0 ]
}
