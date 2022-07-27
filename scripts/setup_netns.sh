#!/bin/bash

set -o errexit  # Used to exit upon error, avoiding cascading errors
set -o pipefail # Unveils hidden failures
set -o nounset  # Exposes unset variables

clean_up(){
    ip netns delete enta_c 2>&- || true
    ip netns delete entg_c 2>&- || true
    ip netns delete entg_s 2>&- || true
    ip netns delete enta_s 2>&- || true

    ip link del eng_p0n0 2>&- || true
    ip link del eng_p1n0 2>&- || true
    ip link del eng_p2n0 2>&- || true
}

clean_up

trap clean_up EXIT

ip netns add enta_c # client-enta node
ip netns add entg_c # client-entg node
ip netns add enta_s # server-entg node
ip netns add entg_s # server-enta node

ip link add eng_p0n0 type veth peer name eng_p0n1
ip link add eng_p1n0 type veth peer name eng_p1n1
ip link add eng_p2n0 type veth peer name eng_p2n1

ip link set eng_p0n0 netns enta_c name eth0
ip link set eng_p0n1 netns entg_c name eth0
ip link set eng_p1n0 netns entg_c name eth1
ip link set eng_p1n1 netns entg_s name eth1
ip link set eng_p2n0 netns entg_s name eth0
ip link set eng_p2n1 netns enta_s name eth0

ip netns exec enta_c ip link set lo up
ip netns exec enta_c ip link set eth0 up
ip netns exec enta_c ip addr add 172.16.0.1/16 dev eth0
ip netns exec enta_c ip route add 172.31.254.1/32 dev eth0

ip netns exec entg_c ip link set lo up
ip netns exec entg_c ip link set eth0 up
ip netns exec entg_c ip addr add 172.31.254.1/16 dev eth0
ip netns exec entg_c ip route add 172.16.0.1/32 dev eth0
ip netns exec entg_c ip link set eth1 up
ip netns exec entg_c ip addr add 10.0.0.1/8 dev eth1 # simplify

ip netns exec entg_s ip link set lo up
ip netns exec entg_s ip link set eth0 up
ip netns exec entg_s ip addr add 172.31.254.1/16 dev eth0
ip netns exec entg_s ip route add 172.16.0.1/32 dev eth0
ip netns exec entg_s ip link set eth1 up
ip netns exec entg_s ip addr add 10.0.0.2/8 dev eth1 # simplify

ip netns exec enta_s ip link set lo up
ip netns exec enta_s ip link set eth0 up
ip netns exec enta_s ip addr add 172.16.0.1/16 dev eth0
ip netns exec enta_s ip route add 172.31.254.1/32 dev eth0


trap EXIT

