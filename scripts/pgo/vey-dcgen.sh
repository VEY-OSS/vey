#!/bin/sh
set -e

COMPONENT=vey-dcgen
HELPERS="vey-mkcert vey-proxy vey-proxy-ctl vey-proxy-ftp vey-iploc vey-statsd"
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
RUN_SH="${SCRIPT_DIR}/../testcases/vey-proxy/run.sh"
. "${SCRIPT_DIR}/enter.sh"
