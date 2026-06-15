
for port in 2443 8443
do

# GET

URL=https://httpbin.local:${port}/get

vey_bench h3 "${URL}" --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"

vey_bench h3 "${URL}" -H "Accept: application/json" --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"

vey_bench h3 "${URL}" -x socks5h://t1:toor@vey-proxy.local:1080 --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"

# POST

URL=https://httpbin.local:${port}/post

vey_bench h3 "${URL}" --method POST --payload 31323334 --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"
vey_bench h3 "${URL}" --method POST --payload 31323334 --binary --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"
vey_bench h3 "${URL}" --method POST --payload name=foo -H "Content-Type: application/x-www-form-urlencoded" --ok-status 200 --tls-ca-cert "${TEST_CA_CERT_FILE}"

done
