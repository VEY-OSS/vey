#!/bin/sh

set -e

SCRIPT_DIR=$(dirname $0)

cd "${SCRIPT_DIR}"

MKCERT="../../../target/debug/vey-mkcert"

$MKCERT --root --common-name "VEY root" --output-cert rootCA.pem --output-key rootCA-key.pem

$MKCERT --tls-server --ca-cert rootCA.pem --ca-key rootCA-key.pem --host vey-proxy.local --output-cert vey-proxy.local.pem --output-key vey-proxy.local-key.pem
$MKCERT --tls-server --ca-cert rootCA.pem --ca-key rootCA-key.pem --host httpbin.local --output-cert httpbin.local.pem --output-key httpbin.local-key.pem
