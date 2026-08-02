
# Dns over UDP, via Cloudflare Public DNS
vey_bench dns "1.1.1.1" www.example.com,A --dump-result

# Dns over TCP, via Cloudflare Public DNS
vey_bench dns "1.1.1.1" --tcp www.example.com,A --dump-result

# Dns over TLS, via Cloudflare Public DNS
vey_bench dns "1.1.1.1" -e dot www.example.com,A --dump-result

# Dns over Https, via Cloudflare Public DNS
vey_bench dns "1.1.1.1" -e doh www.example.com,A --dump-result

# Dns over Quic, via AdGuard Public DNS
vey_bench dns "94.140.14.140" -e doq www.example.com,A --dump-result

# Dns over Http/3, via AdGuard Public DNS
vey_bench dns "94.140.14.140" -e doh3 www.example.com,A --dump-result

# JSON result

JSON_FILE="${JSON_OUT_DIR}/dns-tcp.json"
vey_bench dns "1.1.1.1" --tcp www.example.com,A -n 2 -c 1 --json-file "${JSON_FILE}"
assert_json_report "${JSON_FILE}" dns 1 2
assert_json_type "${JSON_FILE}" .connections object
assert_json_null "${JSON_FILE}" .tls
assert_json_null "${JSON_FILE}" .histograms.conn_used_times
assert_json_null "${JSON_FILE}" .histograms.durations_ns.connect
