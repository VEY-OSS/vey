#!/bin/sh

set -e

SCRIPTS_DIR=$(dirname "$0")
PROJECT_DIR=$(realpath "${SCRIPTS_DIR}/../..")


TEST_NAME="vey-keyless-ci"
. "${SCRIPTS_DIR}/enter.sh"

# build
cargo build --features openssl-async-job -p vey-keyless -p vey-keyless-ctl -p vey-mkcert -p vey-statsd -p vey-bench

all_binaries=$(find target/debug/ -maxdepth 1 -type f -perm /111 | awk '{print "-object "$0}')
all_objects=$(find target/debug/deps/ -type f -perm /111 -not -name "*.so" | awk '{print "-object "$0}')

# run vey-keyless tests

cargo test -p vey-keyless -p vey-keyless-ctl

RUN_DIR="${SCRIPTS_DIR}/vey-keyless"
. "${RUN_DIR}/run.sh"

# get all profraw files generated in each test
profraw_files=$(find . -type f -regex ".*/${TEST_NAME}.*\.profraw")

# get indexed profile data file
cargo profdata -- merge -o "${PROF_DATA_FILE}" ${profraw_files}

# report to console

IGNORE_FLAGS="--ignore-filename-regex=.cargo \
    --ignore-filename-regex=rustc \
    --ignore-filename-regex=target/debug/build \
    --ignore-filename-regex=vey-dcgen \
    --ignore-filename-regex=vey-iploc \
    --ignore-filename-regex=vey-mkcert \
    --ignore-filename-regex=vey-proxy \
    --ignore-filename-regex=vey-gateway"

echo "==== Coverage for all ===="
cargo cov -- report --use-color --instr-profile="${PROF_DATA_FILE}" ${IGNORE_FLAGS} ${all_binaries} ${all_objects}
cargo cov -- export --format=lcov --instr-profile="${PROF_DATA_FILE}" ${IGNORE_FLAGS} ${all_binaries} ${all_objects} > output.lcov
