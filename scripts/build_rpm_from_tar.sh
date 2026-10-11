#!/bin/sh

set -e

SCRIPTS_DIR=$(dirname "$0")
PROJECT_DIR=$(realpath "${SCRIPTS_DIR}/..")

cd "${PROJECT_DIR}"

. "${PROJECT_DIR}/scripts/package/pgo_profile_env.sh"
pgo_profile_env "${PROJECT_DIR}"

export RUSTFLAGS="--remap-path-prefix ${HOME}=~"

if [ -n "${VEY_PGO_PROFILE:-}" ]; then
	rpmbuild -bb --build-in-place \
		--define "vey_pgo_profile ${VEY_PGO_PROFILE}" \
		*.spec
else
	rpmbuild -bb --build-in-place *.spec
fi
