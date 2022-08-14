FROM occlum/occlum:0.28.0-ubuntu20.04

USER root

RUN apt-get update \
        && apt-get install -y iproute2 tmux iptables \
        && update-alternatives --set iptables /usr/sbin/iptables-nft \
        && update-alternatives --set ip6tables /usr/sbin/ip6tables-nft

COPY . /root/enclave-network-gateway

WORKDIR /root/enclave-network-gateway

RUN git submodule update --init

RUN make build && make echosvr

ENTRYPOINT make demo
