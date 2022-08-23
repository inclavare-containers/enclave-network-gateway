
# Design and Implementation

## About the Code

This project is implemented in Rust language. The source code of ENTA and ENTG are located in `enta` and `entg` directories, in the form of rust workspace.

## ENTA

In the current design, the ENTA's responsibility is to capture packets and forward them to the ENTG.

The implementation of ENTA utilizes Rust asynchronous programming features, and our implementation is based on `tokio`, a well-known async runtime.

Currently, ENTA can capture data via TUN device. The captured packet is defined as [`struct ENPacket`](../enta/src/packet.rs), which is currently just a wrapper for the IP Packet, additional fields can be added to it in future.


In order to reduce coupling, as well as make it easier to introduce more capture approaches later, the captured packets are put into a [channel](https://docs.rs/tokio/1.20.1/tokio/sync/mpsc/fn.channel.html) and then forwarded to ENTG uniformly.

Since TCP connections are byte-stream oriented and `ENPacket` is frame-by-frame, when sending the `ENPacket` to the ENTG via byte stream, there must be a way to split the frames. To make it simple, we utilize the [LengthDelimitedCodec](https://docs.rs/tokio-util/latest/tokio_util/codec/length_delimited/) struct in `tokio_util`, which is implemented by adding the length of the frame at the top of each frame (each ENPacket).

### Capturing packets

Currently ENTA supports capturing packets from Host APP with TUN device. For ease of illustration, we refer to the ENTA on the APP Client side as the ENTA Client and the ENTA on the APP Server side as the ENTA Server. assume that the dport of TCP packet expected to be captured is 7.

The capturing approach requires the collaboration of both side.
1. Both ENTA Client and ENTA Server create TUN devices and initialize the IP addresses as `192.168.0.1/24` and `192.168.0.254/24` respectively.
2. The ENTA Client configures iptables to forward the connection with dport 7 to destination `192.168.0.254:6978` via DNAT. The ENTA Server also configures the iptables to REDIRECT the packet with dport 6978 back to 7.
3. To avoid incorrect sip in DNAT packets, the ENTA Client also needs to add a policy-based routing (ip rule) to fix the sip to `192.168.0.1`.

For details, refer to [source code](../enta/src/capture/tun.rs).

## ENTG

ENTG is responsible for forwarding packets between ENTAs.

> In the current implementation, each ENTG connects to an ENTA as well as another ENTG and forwards packets between them.

Like the ENTA, the ENTG also makes use of Rust's asynchronous programming features. Its current implementation is more abbreviated than that of ENTA: since it currently only forwards traffic between ENTA and another ENTG and does not involve routing between multiple parties, it only forwards data flows transparently and does not handle `ENPacket`.

### entg-host & entg-occlum

In the current design, ENTG has two build targets: entg-host and entg-occlum. the former is suitable for running locally and the latter is suitable for running in an occlum environment. This is achieved via two features: `host`, `occlum` in [Cargo.toml](../entg/Cargo.toml).

## rats-tls

Both ENTA and ENTG can use the optional rats-tls connection to replace the normal tcp connection. For this purpose we designed rats-tls, a crate, to call `librats_tls.so` from Rust by way of ffi.

The API interface provided by librats_tls is synchronous blocking IO. To combine it with asynchronous code in ENTG, we also designed the `RatsTls::negotiate_async()` function. It exposes an asynchronous interface. In the internal it will spawn two tokio blocking threads in which to run `rats_tls_receive()` and `rats_tls_transmit()`. The advantage of this design is that it has the same interface as a normal TCP connection (`TCPStream`). Both implement `tokio::io::AsyncRead` and `tokio::io::AsyncWrite`. By using the trait object feature in Rust, the connection type can be eliminated from the logic of data stream forwarding.

## examples

- examples/echosvr

    A simple tcp echo program that can be used to test the correctness of a link.
