#!/bin/sh

# Run cargo, appending -Cprofile-use when a profile is available.
# See scripts/package/pgo_rustflags.sh and doc/rust-pgo-guide.md.

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
flags=$(sh "${SCRIPT_DIR}/pgo_rustflags.sh" "$@")
if [ -n "${flags}" ]; then
	kept=$(printf '%s\n' "${RUSTFLAGS:-}" | awk '
		{
			for (i = 1; i <= NF; i++) {
				if ($i ~ /^-Cprofile-generate/ || $i ~ /^-Cprofile-use/)
					continue
				printf "%s%s", sep, $i
				sep = " "
			}
		}
	')
	export RUSTFLAGS="${kept} ${flags}"
fi
exec cargo "$@"
