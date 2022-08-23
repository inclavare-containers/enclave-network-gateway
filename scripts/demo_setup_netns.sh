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
    ip link del entg_s_out 2>&- || true

    iptables -t nat -D POSTROUTING -s 192.168.254.2/30 '!' -o entg_s_out -j MASQUERADE 2>&- || true
    iptables -t filter -D FORWARD -i any -o entg_s_out -j ACCEPT 2>&- || true
    iptables -t filter -D FORWARD -i entg_s_out -o entg_s_out -j ACCEPT 2>&- || true
    iptables -t filter -D FORWARD -i entg_s_out '!' -o entg_s_out -j ACCEPT 2>&- || true

    ip link del entg_c_out 2>&- || true

    iptables -t nat -D POSTROUTING -s 192.168.254.6/30 '!' -o entg_c_out -j MASQUERADE 2>&- || true
    iptables -t filter -D FORWARD -i any -o entg_c_out -j ACCEPT 2>&- || true
    iptables -t filter -D FORWARD -i entg_c_out -o entg_c_out -j ACCEPT 2>&- || true
    iptables -t filter -D FORWARD -i entg_c_out '!' -o entg_c_out -j ACCEPT 2>&- || true

   ip link del enta_s_out 2>&- || true

    iptables -t nat -D POSTROUTING -s 192.168.254.10/30 '!' -o enta_s_out -j MASQUERADE 2>&- || true
    iptables -t filter -D FORWARD -i any -o enta_s_out -j ACCEPT 2>&- || true
    iptables -t filter -D FORWARD -i enta_s_out -o enta_s_out -j ACCEPT 2>&- || true
    iptables -t filter -D FORWARD -i enta_s_out '!' -o enta_s_out -j ACCEPT 2>&- || true

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
# internet for entg_c
ip link add entg_c_out type veth peer name entg_c_in
ip link set entg_c_in netns entg_c name entg_c_in
ip netns exec entg_c ip link set entg_c_in up
ip netns exec entg_c ip addr add 192.168.254.6/30 dev entg_c_in
ip netns exec entg_c ip route add default via 192.168.254.5
ip link set entg_c_out up
ip addr add 192.168.254.5/30 dev entg_c_out
echo 1 > /proc/sys/net/ipv4/ip_forward
iptables -t nat -A POSTROUTING -s 192.168.254.6/30 '!' -o entg_c_out -j MASQUERADE
iptables -t filter -A FORWARD -i any -o entg_c_out -j ACCEPT
iptables -t filter -A FORWARD -i entg_c_out -o entg_c_out -j ACCEPT
iptables -t filter -A FORWARD -i entg_c_out '!' -o entg_c_out -j ACCEPT

ip netns exec entg_s ip link set lo up
ip netns exec entg_s ip link set eth0 up
ip netns exec entg_s ip addr add 172.31.254.1/16 dev eth0
ip netns exec entg_s ip route add 172.16.0.1/32 dev eth0
ip netns exec entg_s ip link set eth1 up
ip netns exec entg_s ip addr add 10.0.0.2/8 dev eth1 # simplify
# internet for entg_s
ip link add entg_s_out type veth peer name entg_s_in
ip link set entg_s_in netns entg_s name entg_s_in
ip netns exec entg_s ip link set entg_s_in up
ip netns exec entg_s ip addr add 192.168.254.2/30 dev entg_s_in
ip netns exec entg_s ip route add default via 192.168.254.1
ip link set entg_s_out up
ip addr add 192.168.254.1/30 dev entg_s_out
echo 1 > /proc/sys/net/ipv4/ip_forward
iptables -t nat -A POSTROUTING -s 192.168.254.2/30 '!' -o entg_s_out -j MASQUERADE
iptables -t filter -A FORWARD -i any -o entg_s_out -j ACCEPT
iptables -t filter -A FORWARD -i entg_s_out -o entg_s_out -j ACCEPT
iptables -t filter -A FORWARD -i entg_s_out '!' -o entg_s_out -j ACCEPT


ip netns exec enta_s ip link set lo up
ip netns exec enta_s ip link set eth0 up
ip netns exec enta_s ip addr add 172.16.0.1/16 dev eth0
ip netns exec enta_s ip route add 172.31.254.1/32 dev eth0
# internet for enta_s
ip link add enta_s_out type veth peer name enta_s_in
ip link set enta_s_in netns enta_s name enta_s_in
ip netns exec enta_s ip link set enta_s_in up
ip netns exec enta_s ip addr add 192.168.254.10/30 dev enta_s_in
ip netns exec enta_s ip route add default via 192.168.254.9
ip link set enta_s_out up
ip addr add 192.168.254.9/30 dev enta_s_out
echo 1 > /proc/sys/net/ipv4/ip_forward
iptables -t nat -A POSTROUTING -s 192.168.254.10/30 '!' -o enta_s_out -j MASQUERADE
iptables -t filter -A FORWARD -i any -o enta_s_out -j ACCEPT
iptables -t filter -A FORWARD -i enta_s_out -o enta_s_out -j ACCEPT
iptables -t filter -A FORWARD -i enta_s_out '!' -o enta_s_out -j ACCEPT


trap EXIT

