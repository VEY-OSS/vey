#!/bin/sh
set -e

COMPONENT=vey-proxy
HELPERS="vey-mkcert vey-dcgen vey-iploc vey-statsd vey-proxy-ctl vey-proxy-ftp"
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
RUN_SH="${SCRIPT_DIR}/../testcases/vey-proxy/run.sh"
. "${SCRIPT_DIR}/enter.sh"
