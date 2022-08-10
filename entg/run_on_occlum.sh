#!/bin/bash
set -e

BASE_DIR="$(realpath $(dirname $0))"

pushd "$BASE_DIR"

# initialize occlum workspace
rm -rf occlum_instance && mkdir occlum_instance && cd occlum_instance

occlum init && rm -rf image
# enlarge .resource_limits.kernel_space_heap_size
# enlarge .resource_limits.max_num_of_threads
cp Occlum.json Occlum.json.bak && jq '.resource_limits.kernel_space_heap_size = "64MB" | .resource_limits.max_num_of_threads = 64' Occlum.json.bak > Occlum.json && rm Occlum.json.bak

# TODO: Fix bug that copy file to `entg/occlum_instance/image/root/enclave-network-gateway/deps/rats-tls/build-occlum/src/librats_tls.so.0`, which is caused by RUNPATH of lib*.so
# Note: set OCCLUM_LOG_LEVEL=trace to show more log of `copy_bom`
copy_bom -f ../occlum.yaml --root image --include-dir /opt/occlum/etc/template

occlum build
occlum run /bin/entg-occlum $@
popd
