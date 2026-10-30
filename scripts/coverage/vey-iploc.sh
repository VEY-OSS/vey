#!/bin/sh

set -e

SCRIPTS_DIR=$(dirname "$0")
PROJECT_DIR=$(realpath "${SCRIPTS_DIR}/../..")


TEST_NAME="vey-iploc-ci"
. "${SCRIPTS_DIR}/enter.sh"

# build
FEATURES="jemalloc"
if command -v clang >/dev/null 2>&1 && pkg-config --exists libbpf; then
	FEATURES="${FEATURES},ebpf"
fi
cargo build --features "${FEATURES}" -p vey-iploc -p vey-iploc-ctl

all_binaries=$(find target/debug/ -maxdepth 1 -type f -perm /111 | awk '{print "-object "$0}')
all_objects=$(find target/debug/deps/ -type f -perm /111 -not -name "*.so" | awk '{print "-object "$0}')

# run vey-iploc tests

cargo test --features "${FEATURES}" -p vey-iploc -p vey-iploc-ctl

RUN_DIR="${PROJECT_DIR}/scripts/testcases/vey-iploc"
. "${RUN_DIR}/run.sh"

# get all profraw files generated in each test
profraw_files=$(find . -type f -regex ".*/${TEST_NAME}.*\.profraw")

# get indexed profile data file
cargo profdata -- merge -o "${PROF_DATA_FILE}" ${profraw_files}

# report to console

IGNORE_FLAGS="--ignore-filename-regex=.cargo \
    --ignore-filename-regex=rustc \
    --ignore-filename-regex=target/debug/build \
    --ignore-filename-regex=vey-bench \
    --ignore-filename-regex=vey-dcgen \
    --ignore-filename-regex=vey-mkcert \
    --ignore-filename-regex=vey-proxy \
    --ignore-filename-regex=vey-gateway \
    --ignore-filename-regex=vey-keyless \
    --ignore-filename-regex=vey-statsd"

echo "==== Coverage for all ===="
cargo cov -- report --use-color --instr-profile="${PROF_DATA_FILE}" ${IGNORE_FLAGS} ${all_binaries} ${all_objects}
cargo cov -- export --format=lcov --instr-profile="${PROF_DATA_FILE}" ${IGNORE_FLAGS} ${all_binaries} ${all_objects} > output.lcov
