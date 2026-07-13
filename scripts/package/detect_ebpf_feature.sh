#!/bin/sh

set -e

if pkg-config --atleast-version 1.5.0 libbpf
then
	echo "ebpf"
elif pkg-config --atleast-version 1.4.6 --max-version 1.4.100 libbpf
then
  echo "ebpf"
elif pkg-config --atleast-version 1.3.3 --max-version 1.3.100 libbpf
then
  echo "ebpf"
elif pkg-config --atleast-version 1.2.3 --max-version 1.2.100 libbpf
then
  echo "ebpf"
elif pkg-config --atleast-version 1.1.2 --max-version 1.1.100 libbpf
then
  echo "ebpf"
else
	echo "vendored-ebpf"
fi
