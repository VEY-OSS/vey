#!/bin/sh

set -e

SCRIPTS_DIR=$(dirname "$0")
PROJECT_DIR=$(realpath "${SCRIPTS_DIR}/../..")


TEST_NAME="reuseport-test"
. "${SCRIPTS_DIR}/enter.sh"

# build
cargo build -p vey-reuseport

all_binaries=$(find target/debug/ -maxdepth 1 -type f -perm /111 | awk '{print "-object "$0}')

# run the tests
sudo --preserve-env=LLVM_PROFILE_FILE "${PROJECT_DIR}"/target/debug/udp_reuseport_test
sudo --preserve-env=LLVM_PROFILE_FILE "${PROJECT_DIR}"/target/debug/tcp_reuseport_test

all_objects=$(find target/debug/deps/ -type f -perm /111 -not -name "*.so" | awk '{print "-object "$0}')


# get all profraw files generated in each test
profraw_files=$(find . -type f -regex ".*/${TEST_NAME}.*\.profraw")

# get indexed profile data file
cargo profdata -- merge -o "${PROF_DATA_FILE}" ${profraw_files}

# report to console

IGNORE_FLAGS="--ignore-filename-regex=.cargo \
    --ignore-filename-regex=rustc \
    --ignore-filename-regex=target/debug/build"

cargo cov -- report --use-color --instr-profile="${PROF_DATA_FILE}" ${IGNORE_FLAGS} ${all_binaries} ${all_objects}
cargo cov -- export --format=lcov --instr-profile="${PROF_DATA_FILE}" ${IGNORE_FLAGS} ${all_binaries} ${all_objects} > output.lcov
