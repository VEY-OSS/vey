#!/bin/sh

# Print extra rustc flags when a PGO profile should be used.
# Stdout is either empty or: -Cprofile-use=<absolute path>
#
# Opt in by either:
#   - setting VEY_PGO_PROFILE to a profile path (absolute, or relative to the repo root), or
#   - leaving scripts/pgo/profile/<package>.profdata in the source tree
# The package name comes from -p / --package in the arguments (the cargo
# command). The first listed package that has such a file is used.
# VEY_PGO_PROFILE wins. With neither, this prints nothing and packaging stays
# a normal build.

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
ROOT=$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd)

profile=${VEY_PGO_PROFILE:-}
if [ -z "${profile}" ]; then
	prev=
	for arg in "$@"; do
		name=
		case ${prev} in
		-p | --package) name=${arg} ;;
		esac
		case ${arg} in
		--package=*) name=${arg#--package=} ;;
		esac
		case ${arg} in
		-p | --package) prev=${arg} ;;
		*) prev= ;;
		esac
		if [ -n "${name}" ] && [ -f "${ROOT}/scripts/pgo/profile/${name}.profdata" ]; then
			profile="${ROOT}/scripts/pgo/profile/${name}.profdata"
			break
		fi
	done
	if [ -z "${profile}" ]; then
		exit 0
	fi
fi

case ${profile} in
/*) ;;
*) profile="${ROOT}/${profile}" ;;
esac

if [ ! -f "${profile}" ]; then
	echo "VEY_PGO_ERROR: profile not found: ${profile}" >&2
	exit 1
fi

echo "using PGO profile ${profile}" >&2
printf '%s\n' "-Cprofile-use=${profile}"
