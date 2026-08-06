#!/bin/sh

set -e

if pkg-config --atleast-version 1.5.0 libbpf
then
	echo "ebpf"
else
	echo "vendored-libbpf"
fi
