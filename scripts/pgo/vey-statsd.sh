#!/bin/sh
set -e

COMPONENT=vey-statsd
HELPERS="vey-statsd-ctl"
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
RUN_SH="${SCRIPT_DIR}/../testcases/vey-statsd/run.sh"
. "${SCRIPT_DIR}/enter.sh"
