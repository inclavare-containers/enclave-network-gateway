#!/bin/bash
set -e

BASE_DIR="$(realpath $(dirname $0))"

pushd "$BASE_DIR"

# compile
occlum-cargo build --package entg

# initialize occlum workspace
rm -rf occlum_instance && mkdir occlum_instance && cd occlum_instance

occlum init && rm -rf image
# enlarge .resource_limits.kernel_space_heap_size
# enlarge .resource_limits.max_num_of_threads
cp Occlum.json Occlum.json.bak && jq '.resource_limits.kernel_space_heap_size = "64MB" | .resource_limits.max_num_of_threads = 64' Occlum.json.bak > Occlum.json && rm Occlum.json.bak

copy_bom -f ../occlum.yaml --root image --include-dir /opt/occlum/etc/template

occlum build
occlum run /bin/entg $@
popd
