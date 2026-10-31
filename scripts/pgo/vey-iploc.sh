#!/bin/sh
set -e

COMPONENT=vey-iploc
HELPERS="vey-iploc-ctl"
FEATURES="jemalloc"
if command -v clang >/dev/null 2>&1 && pkg-config --exists libbpf; then
	FEATURES="${FEATURES},ebpf"
fi
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
RUN_SH="${SCRIPT_DIR}/../testcases/vey-iploc/run.sh"
. "${SCRIPT_DIR}/enter.sh"
