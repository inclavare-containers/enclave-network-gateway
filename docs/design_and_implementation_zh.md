
# 设计与实现

## 源码结构

本项目使用Rust语言实现，ENTA和ENTG的源码分别位于`enta`和`entg`目录下，整体用rust workspace的形式组织。

## ENTA

在目前的设计中，ENTA的职责主要是捕获数据包并将其转发交给ENTG。

ENTA的代码实现中使用了Rust异步编程特性，我们的实现是基于tokio这一Async Runtime的。

目前ENTA中实现了通过TUN设备捕获数据。捕获的数据包被定义为[`struct ENPacket`](../enta/src/packet.rs)，目前只是对IP Packet的一个包装，后续可以考虑在其中加入额外的字段。

为了降低代码耦合性，以及便于后续引入更多的捕获方式，捕获的数据包会被放入到一个[channel](https://docs.rs/tokio/1.20.1/tokio/sync/mpsc/fn.channel.html)中，再统一转发到ENTG。

由于TCP连接是面向字节流的，而`ENPacket`是逐帧（Frame）的，在将`ENPacket`通过字节流发送给ENTG时，必须要采取一种方式进行分帧。简单起见我们使用了tokio_util中的[LengthDelimitedCodec](https://docs.rs/tokio-util/latest/tokio_util/codec/length_delimited/)模式，它的实现是在每一帧（每个ENPacket）的最前面添加帧的长度。

### 数据包捕获

目前ENTA支持使用TUN设备捕获Host APP的数据包。为了便于说明，我们将APP Client侧的ENTA称为ENTA Client，将APP Server侧的ENTA称为ENTA Server。假设期望捕获的TCP数据包dport为7。

这种捕获方式需要两者协同进行：
1. ENTA Client和ENTA Server均创建TUN设备，将IP地址分别初始化为`192.168.0.1/24`和`192.168.0.254/24`。
2. ENTA Client端编辑iptables配置，利用DNAT将dport为7的连接转发到`192.168.0.254:6978`。ENTA Server端也编辑iptables配置，将dport为6978的数据包REDIRECT回7。
3. 为了解决DNAT的数据包中sip不正确，导致数据包回程出错的问题，ENTA Client还需要增加策略路由（ip rule），使得sip固定为`192.168.0.1`。

具体参考[源码](../enta/src/capture/tun.rs)

## ENTG

ENTG负责对ENTA的数据包进行转发。

> 目前的的实现中，每个ENTG会连接到一个ENTA以及另一个ENTG，在两者之间转发数据包，后续也可修改逻辑以实现在多于两个连接方之间流转。

与ENTA一样，ENTG也使用Rust的异步编程特性。目前它的实现相比ENTA更为简略：由于目前只负责在ENTA与另一个ENTG之间转发流量，不涉及多方的路由，因此只实现了数据流转发，而不涉及`ENPacket`的解析。

### entg-host & entg-occlum

在目前的设计中，ENTG有两个编译目标：entg-host和entg-occlum。前者适合在本机运行，后者适合在occlum环境中运行。这是通过[Cargo.toml](../entg/Cargo.toml)中的两个features：`host`、`occlum`控制的。

## rats-tls

ENTA和ENTG都允许使用rats-tls连接替代普通的tcp连接。为此我们设计了rats-tls这个crate，通过ffi的方式从Rust中调用`librats_tls.so`。

由于librats_tls提供的API接口为同步阻塞IO，为了与ENTG的异步代码结合，我们还设计了`RatsTls::negotiate_async()`函数。它会spawn出两个tokio的阻塞线程（blocking thread），在其中中执行`rats_tls_receive()`和`rats_tls_transmit()`操作，并对外暴露出异步的接口。这种设计的好处是，普通的TCP连接（`TCPStream`）和rats-tls连接具有一样的接口（都实现了`tokio::io::AsyncRead`和`tokio::io::AsyncWrite`），借助trait object特性，在数据流转发的实现中便可无需考虑底层具体的连接类型。

## examples

- examples/echosvr

    一个简易的 tcp echo 程序，可以用于测试链路的正确性。
