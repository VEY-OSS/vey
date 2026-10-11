#!/bin/sh
set -e

COMPONENT=vey-keyless
FEATURES="mimalloc,openssl-async-job"
HELPERS="vey-mkcert vey-statsd vey-keyless-ctl vey-bench"
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
RUN_SH="${SCRIPT_DIR}/../testcases/vey-keyless/run.sh"
. "${SCRIPT_DIR}/enter.sh"
