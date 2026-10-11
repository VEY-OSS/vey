# Resolve VEY_PGO_PROFILE for package and image builds.
# Source this file, then call: pgo_profile_env /path/to/repo
#
# Relative paths are relative to the repository root. When the variable is
# set, it is replaced with an absolute path and exported. When it is unset,
# this does nothing and the build stays a normal build.

pgo_profile_env() {
	root=$1
	if [ -z "${VEY_PGO_PROFILE:-}" ]; then
		return 0
	fi
	case ${VEY_PGO_PROFILE} in
	/*) ;;
	*) VEY_PGO_PROFILE="${root}/${VEY_PGO_PROFILE}" ;;
	esac
	if [ ! -f "${VEY_PGO_PROFILE}" ]; then
		echo "VEY_PGO_PROFILE not found: ${VEY_PGO_PROFILE}" >&2
		return 1
	fi
	export VEY_PGO_PROFILE
}
