#!/bin/sh

set -e

SCRIPT_DIR=$(dirname $0)

cd "${SCRIPT_DIR}"

MKCERT="../../../target/debug/vey-mkcert"

$MKCERT --root --common-name "VEY root" --output-cert rootCA.pem --output-key rootCA-key.pem

for bits in 2048 3072 4096
do
	$MKCERT --tls-server --ca-cert rootCA.pem --ca-key rootCA-key.pem --host vey-proxy.local --rsa "${bits}" --output-cert "rsa${bits}.crt" --output-key "rsa${bits}.key"
done

for curve in ec256 ec384 ec521
do
	$MKCERT --tls-server --ca-cert rootCA.pem --ca-key rootCA-key.pem --host vey-proxy.local --"${curve}" --output-cert "${curve}.crt" --output-key "${curve}.key"
done

$MKCERT --tls-server --ca-cert rootCA.pem --ca-key rootCA-key.pem --host vey-proxy.local --ed25519 --output-cert ed25519.crt --output-key ed25519.key

mv *.key keys/
