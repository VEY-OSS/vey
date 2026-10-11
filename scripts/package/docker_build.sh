#!/bin/sh

# docker build, with VEY_PGO_PROFILE taken from the environment.
#
# Docker does not pass the host environment into the build, and a profile
# outside the build context is not visible in the image. When VEY_PGO_PROFILE
# is set, this copies that file into the context if needed and passes it as
# the VEY_PGO_PROFILE build arg. Other arguments are forwarded to docker build.
#
# Run from anywhere. The build context should be this repository.

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
ROOT=$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd)

. "${SCRIPT_DIR}/pgo_profile_env.sh"
pgo_profile_env "${ROOT}"

staged=
cleanup() {
	if [ -n "${staged}" ]; then
		rm -f "${staged}"
	fi
}
trap cleanup EXIT

if [ -n "${VEY_PGO_PROFILE:-}" ]; then
	case ${VEY_PGO_PROFILE} in
	"${ROOT}"/*)
		rel=${VEY_PGO_PROFILE#"${ROOT}/"}
		;;
	*)
		rel="scripts/pgo/profile/from-env-$$.profdata"
		staged="${ROOT}/${rel}"
		cp -f "${VEY_PGO_PROFILE}" "${staged}"
		;;
	esac
fi

cd "${ROOT}"
if [ -n "${rel:-}" ]; then
	docker build --build-arg "VEY_PGO_PROFILE=${rel}" "$@"
else
	docker build "$@"
fi
