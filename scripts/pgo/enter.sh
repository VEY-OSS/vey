#!/bin/sh

# Sourced by scripts/pgo/<component>.sh.
# Expects COMPONENT and RUN_SH.
# Optional: FEATURES (cargo --features), HELPERS (package names to build
# without instrumentation, into target/debug).
#
# Builds an instrumented COMPONENT, runs the shared testcases run.sh, and
# writes one .profdata file. Does not source scripts/coverage/enter.sh.

set -e

if [ -z "${COMPONENT:-}" ] || [ -z "${RUN_SH:-}" ]; then
	echo "COMPONENT and RUN_SH must be set" >&2
	exit 1
fi

if ! cargo profdata --help >/dev/null 2>&1; then
	echo "cargo profdata not found. Install cargo-binutils (doc/dev-setup.md)." >&2
	exit 1
fi

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
PROJECT_DIR=$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd)
RUN_DIR=$(CDPATH= cd -- "$(dirname "${RUN_SH}")" && pwd)
OUT=${VEY_PGO_OUT:-${SCRIPT_DIR}/profile/${COMPONENT}.profdata}
TARGET_DIR=${PROJECT_DIR}/target/pgo-generate
CARGO_PROFILE=release-lto
HELPER_BIN_DIR=${VEY_HELPER_BIN_DIR:-${PROJECT_DIR}/target/debug}
TEST_NAME=${TEST_NAME:-pgo-${COMPONENT}}

case ${OUT} in
/*) ;;
*) OUT="${PROJECT_DIR}/${OUT}" ;;
esac

strip_pgo_rustflags() {
	printf '%s\n' "${RUSTFLAGS:-}" | awk '
		{
			for (i = 1; i <= NF; i++) {
				if ($i ~ /^-Cprofile-generate/ || $i ~ /^-Cprofile-use/)
					continue
				printf "%s%s", sep, $i
				sep = " "
			}
		}
	'
}

saved_rustflags=${RUSTFLAGS:-}
saved_target_dir=${CARGO_TARGET_DIR:-}
base_rustflags=$(strip_pgo_rustflags)

cd "${PROJECT_DIR}"

if [ -n "${HELPERS:-}" ]; then
	echo "building helpers: ${HELPERS}"
	unset CARGO_TARGET_DIR
	export RUSTFLAGS="${base_rustflags}"
	helper_args=
	for pkg in ${HELPERS}; do
		helper_args="${helper_args} -p ${pkg}"
	done
	# package names have no spaces
	# shellcheck disable=SC2086
	cargo build ${helper_args}
fi

PGO_RAW_DIR=$(mktemp -d)
RAW_DIR=${PGO_RAW_DIR}
trap 'rm -rf "${PGO_BIN_DIR:-}" "${PGO_RAW_DIR:-}"' EXIT
export CARGO_TARGET_DIR="${TARGET_DIR}"
export RUSTFLAGS="${base_rustflags} -Cprofile-generate=${RAW_DIR}"
echo "instrumented build: ${COMPONENT} (profile ${CARGO_PROFILE})"
if [ -n "${FEATURES:-}" ]; then
	cargo build --profile "${CARGO_PROFILE}" --features "${FEATURES}" -p "${COMPONENT}"
else
	cargo build --profile "${CARGO_PROFILE}" -p "${COMPONENT}"
fi
# Build scripts write samples during compilation. Drop those so the
# merged profile is only the workload.
rm -f "${RAW_DIR}"/*.profraw

INSTRUMENTED="${TARGET_DIR}/${CARGO_PROFILE}/${COMPONENT}"
if [ ! -x "${INSTRUMENTED}" ]; then
	echo "instrumented binary is not executable: ${INSTRUMENTED}" >&2
	exit 1
fi
if [ ! -d "${HELPER_BIN_DIR}" ]; then
	echo "helper binaries not found: ${HELPER_BIN_DIR}" >&2
	exit 1
fi
HELPER_BIN_DIR=$(CDPATH= cd -- "${HELPER_BIN_DIR}" && pwd)

PGO_BIN_DIR=$(mktemp -d)
BIN_DIR=${PGO_BIN_DIR}
for path in "${HELPER_BIN_DIR}"/*; do
	if [ -f "${path}" ] && [ -x "${path}" ]; then
		ln -s "${path}" "${PGO_BIN_DIR}/$(basename "${path}")"
	fi
done
ln -sfn "${INSTRUMENTED}" "${PGO_BIN_DIR}/${COMPONENT}"

export LLVM_PROFILE_FILE="${RAW_DIR}/pgo-%p-%m.profraw"
export PROJECT_DIR RUN_DIR TEST_NAME BIN_DIR PGO_BIN_DIR PGO_RAW_DIR
if [ -n "${saved_target_dir}" ]; then
	export CARGO_TARGET_DIR="${saved_target_dir}"
else
	unset CARGO_TARGET_DIR
fi
export RUSTFLAGS="${saved_rustflags}"

cd "${PROJECT_DIR}"

# shellcheck disable=SC1090
. "${RUN_SH}"

set -- "${RAW_DIR}"/*.profraw
if [ ! -f "$1" ]; then
	echo "no .profraw files in ${RAW_DIR}" >&2
	exit 1
fi
mkdir -p "$(dirname "${OUT}")"
echo "merging $# profile(s) -> ${OUT}"
(
	cd "${PROJECT_DIR}"
	cargo profdata -- merge -o "${OUT}" "$@"
)
echo "wrote ${OUT}"
echo "VEY_PGO_PROFILE=${OUT}"
