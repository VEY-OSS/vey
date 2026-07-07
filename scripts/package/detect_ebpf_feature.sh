#!/bin/sh

set -e

if $(pkg-config --atleast-version 1.0 libbpf)
then
	echo "ebpf"
else
	echo "vendored-ebpf"
fi
