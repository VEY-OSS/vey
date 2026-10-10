#!/bin/sh
# Create a local CA the first time the container starts, then exec vey-dcgen.
# A mounted certificate and key are left untouched.

set -eu

ca_dir=/var/lib/vey-dcgen
ca_cert="${ca_dir}/ca.crt"
ca_key="${ca_dir}/ca.key"

need_ca=1
for arg in "$@"; do
  case "${arg}" in
    -V | --version | -h | --help) need_ca=0 ;;
  esac
done

if [ "${need_ca}" -eq 1 ]; then
  mkdir -p "${ca_dir}"
  if [ ! -s "${ca_cert}" ] || [ ! -s "${ca_key}" ]; then
    if [ -e "${ca_cert}" ] || [ -e "${ca_key}" ]; then
      echo "vey-dcgen: ${ca_cert} and ${ca_key} must both be present and non-empty" >&2
      exit 1
    fi
    vey-mkcert --root --ec256 --common-name vey-dcgen \
      --output-cert "${ca_cert}" \
      --output-key "${ca_key}"
    chmod 600 "${ca_key}"
  fi
fi

exec /usr/bin/vey-dcgen "$@"
