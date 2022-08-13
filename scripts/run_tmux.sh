#!/bin/bash

set -o errexit  # Used to exit upon error, avoiding cascading errors
set -o pipefail # Unveils hidden failures
set -o nounset  # Exposes unset variables

bash -x "$(dirname $0)/setup_netns.sh"

tmux kill-session -t eng 2>&- || true
tmux new -d -s eng
tmux split-window -p 66 \; split-window -d \; split-window -h \; select-pane -t 0 \; split-window -h \; select-pane -t 4 \; split-window -h

tmux send-keys -t eng:0.0 \
    "exec ip netns exec enta_c bash" ENTER \
        'PS1=(enta_c)${PS1}' ENTER \
        "sleep 8.5" ENTER \
        "RUST_LOG=debug $(dirname $0)/../target/debug/enta --mode client --entg-connect 172.31.254.1:6980 --tun-addr 192.168.0.1 --tun-mask 255.255.255.0" ENTER \

tmux send-keys -t eng:0.1 \
    "exec ip netns exec enta_s bash" ENTER \
        'PS1=(enta_s)${PS1}' ENTER \
        "sleep 8" ENTER \
        "RUST_LOG=debug $(dirname $0)/../target/debug/enta --mode server --entg-rats-tls --entg-connect 172.31.254.1:6980 --tun-addr 192.168.0.254 --tun-mask 255.255.255.0" ENTER

tmux send-keys -t eng:0.2 \
    "exec ip netns exec entg_c bash" ENTER \
        'PS1=(entg_c)${PS1}' ENTER \
        "sleep 7" ENTER \
        "$(dirname $0)/../target/debug/entg-host --entg-rats-tls --entg-connect 10.0.0.2:6979 --enta-listen 6980" ENTER
tmux send-keys -t eng:0.3 \
    "exec ip netns exec entg_s bash" ENTER \
        'PS1=(entg_s)${PS1}' ENTER \
        "$(dirname $0)/../entg/run_on_occlum.sh --entg-rats-tls --entg-listen 6979 --enta-rats-tls --enta-listen 6980" ENTER

tmux send-keys -t eng:0.4 \
    "exec ip netns exec enta_c bash" ENTER \
        "sleep 9.5" ENTER \
        "$(dirname $0)/../examples/echosvr/target/debug/echosvr client --host 1.1.1.1 --port 7" ENTER


tmux send-keys -t eng:0.5 \
    "exec ip netns exec enta_s bash" ENTER \
        "sleep 9" ENTER \
        "$(dirname $0)/../examples/echosvr/target/debug/echosvr server --host 0.0.0.0 --port 7" ENTER

tmux a -t eng

