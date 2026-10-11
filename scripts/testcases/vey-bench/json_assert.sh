
# Shared helpers for --json-file result assertions (POSIX sh)

JSON_OUT_DIR="${RUN_DIR}/.json-out"
mkdir -p "${JSON_OUT_DIR}"

assert_json_eq()
{
	_aj_eq_file=$1
	_aj_eq_expr=$2
	_aj_eq_expected=$3
	_aj_eq_actual=$(jq -er "${_aj_eq_expr}" "${_aj_eq_file}")
	if [ "${_aj_eq_actual}" != "${_aj_eq_expected}" ]; then
		echo "JSON assert failed for ${_aj_eq_file}: ${_aj_eq_expr} expected='${_aj_eq_expected}' actual='${_aj_eq_actual}'" >&2
		exit 1
	fi
}

assert_json_gt()
{
	_aj_gt_file=$1
	_aj_gt_expr=$2
	_aj_gt_min=$3
	_aj_gt_actual=$(jq -er "${_aj_gt_expr}" "${_aj_gt_file}")
	if ! [ "${_aj_gt_actual}" -gt "${_aj_gt_min}" ]; then
		echo "JSON assert failed for ${_aj_gt_file}: ${_aj_gt_expr} expected > ${_aj_gt_min} actual='${_aj_gt_actual}'" >&2
		exit 1
	fi
}

assert_json_type()
{
	_aj_ty_file=$1
	_aj_ty_expr=$2
	_aj_ty_type=$3
	_aj_ty_actual=$(jq -er "${_aj_ty_expr} | type" "${_aj_ty_file}")
	if [ "${_aj_ty_actual}" != "${_aj_ty_type}" ]; then
		echo "JSON assert failed for ${_aj_ty_file}: ${_aj_ty_expr} type expected='${_aj_ty_type}' actual='${_aj_ty_actual}'" >&2
		exit 1
	fi
}

assert_json_null()
{
	_aj_nu_file=$1
	_aj_nu_expr=$2
	_aj_nu_actual=$(jq -er "${_aj_nu_expr} | type" "${_aj_nu_file}")
	if [ "${_aj_nu_actual}" != "null" ]; then
		echo "JSON assert failed for ${_aj_nu_file}: ${_aj_nu_expr} expected null actual type='${_aj_nu_actual}'" >&2
		exit 1
	fi
}

assert_hist_snapshot()
{
	_aj_hs_file=$1
	_aj_hs_expr=$2
	assert_json_type "${_aj_hs_file}" "${_aj_hs_expr}.min" number
	assert_json_type "${_aj_hs_file}" "${_aj_hs_expr}.mean" number
	assert_json_type "${_aj_hs_file}" "${_aj_hs_expr}.stdev" number
	assert_json_type "${_aj_hs_file}" "${_aj_hs_expr}.p90" number
	assert_json_type "${_aj_hs_file}" "${_aj_hs_expr}.max" number
}

assert_json_report()
{
	# assert_json_report <file> <target> <concurrency> <complete_requests>
	_aj_rp_file=$1
	_aj_rp_target=$2
	_aj_rp_concurrency=$3
	_aj_rp_complete=$4
	assert_json_eq "${_aj_rp_file}" .version 1
	assert_json_eq "${_aj_rp_file}" .target "${_aj_rp_target}"
	assert_json_eq "${_aj_rp_file}" .concurrency "${_aj_rp_concurrency}"
	assert_json_eq "${_aj_rp_file}" .global.complete_requests "${_aj_rp_complete}"
	assert_json_eq "${_aj_rp_file}" .global.failed_requests 0
	assert_json_gt "${_aj_rp_file}" .global.total_time_ns 0
	assert_json_type "${_aj_rp_file}" .global.requests_per_sec number
	assert_json_type "${_aj_rp_file}" .histograms object
	assert_hist_snapshot "${_aj_rp_file}" .histograms.durations_ns.total
	assert_json_type "${_aj_rp_file}" .percentiles_ns.p50 number
	assert_json_type "${_aj_rp_file}" .percentiles_ns.p100 number
}

assert_json_tcp_traffic()
{
	_aj_tcp_file=$1
	assert_json_type "${_aj_tcp_file}" .traffic.tcp object
	assert_json_gt "${_aj_tcp_file}" .traffic.tcp.send_bytes 0
	assert_json_gt "${_aj_tcp_file}" .traffic.tcp.recv_bytes 0
	assert_json_null "${_aj_tcp_file}" .traffic.udp
}

assert_json_udp_traffic()
{
	_aj_udp_file=$1
	assert_json_type "${_aj_udp_file}" .traffic.udp object
	assert_json_gt "${_aj_udp_file}" .traffic.udp.send_bytes 0
	assert_json_gt "${_aj_udp_file}" .traffic.udp.send_packets 0
	assert_json_gt "${_aj_udp_file}" .traffic.udp.recv_packets 0
	assert_json_null "${_aj_udp_file}" .traffic.tcp
}

assert_json_http_histograms()
{
	_aj_hh_file=$1
	assert_hist_snapshot "${_aj_hh_file}" .histograms.conn_used_times
	assert_hist_snapshot "${_aj_hh_file}" .histograms.durations_ns.connect
	assert_hist_snapshot "${_aj_hh_file}" .histograms.durations_ns.send_hdr
	assert_hist_snapshot "${_aj_hh_file}" .histograms.durations_ns.send_all
	assert_hist_snapshot "${_aj_hh_file}" .histograms.durations_ns.recv_hdr
	assert_hist_snapshot "${_aj_hh_file}" .histograms.durations_ns.total
}
