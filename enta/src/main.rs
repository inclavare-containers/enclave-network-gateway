use std::net::IpAddr;

use anyhow::{Context, Result};
use bytes::Bytes;
use clap::Parser;
use futures::{SinkExt, StreamExt};
use log::{debug, error, info};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::{
    net::TcpStream,
    sync::mpsc::{Receiver, Sender},
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tun::{AsyncDevice, Layer, TunPacket};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Host address and port of ENTG
    #[clap(long, value_parser, default_value = "127.0.0.1:6979")]
    entg_addr: String,

    /// Set address for tun device
    #[clap(long, value_parser, default_value = "192.168.0.1")]
    tun_addr: IpAddr,

    /// Set network mask for tun device
    #[clap(long, value_parser, default_value = "255.255.255.0")]
    tun_mask: IpAddr,
}

type ENPacket = bytes::Bytes;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let args = Args::parse();
    let stream = connect_to_entg(&args.entg_addr).await?;
    let dev = setup_tun(args.tun_addr, args.tun_mask).await?;

    let (tun2entg_sender, tun2entg_receiver) = mpsc::channel(128);
    let (entg2tun_sender, entg2tun_receiver) = mpsc::channel(128);
    let handle1 = exchange_with_entg(stream, entg2tun_sender, tun2entg_receiver).await?;
    let handle2 = exchange_with_tun(dev, tun2entg_sender, entg2tun_receiver).await?;

    let (_first, _second) = tokio::join!(handle1, handle2);
    Ok(())
}

async fn connect_to_entg(entg_addr: &str) -> Result<TcpStream> {
    info!("Connecting to ENTG");
    let stream = TcpStream::connect(entg_addr)
        .await
        .with_context(|| format!("Falied to connect to ENTG: {}", entg_addr))?;
    info!(
        "Connection with ENTG is established, peer address: {}",
        stream.peer_addr()?
    );
    Ok(stream)
}

async fn exchange_with_entg(
    stream: TcpStream,
    entg2tun_sender: Sender<ENPacket>,
    mut tun2entg_receiver: Receiver<ENPacket>,
) -> Result<JoinHandle<()>> {
    let (mut split_sink, mut split_stream) =
        Framed::new(stream, LengthDelimitedCodec::new()).split();
    let to_entg = async move {
        loop {
            match tun2entg_receiver.recv().await {
                Some(packet) => {
                    if let Err(e) = split_sink.send(packet.into()).await {
                        error!("Failed to send data to ENTG: {}", e);
                        break;
                    }
                }
                None => {
                    info!("tun2entg channel closed, shutdown connection to ENTG");
                    if let Err(e) = split_sink.close().await {
                        error!("Failed to shutdown connection with ENTG: {}", e);
                    }
                    break;
                }
            }
        }
        // Clean up
        drop(tun2entg_receiver);
        if let Err(e) = split_sink.close().await {
            error!("Failed to shutdown connection with ENTG: {}", e);
        }
    };

    let from_entg = async move {
        loop {
            match split_stream.next().await {
                Some(Ok(packet)) => {
                    if let Err(e) = entg2tun_sender.send(packet.freeze()).await {
                        info!(
                            "entg2tun channel closed, shutdown connection to ENTG: {}",
                            e
                        );
                        break;
                    }
                }
                Some(Err(e)) => {
                    error!("Failed to receive data from ENTG: {}", e);
                }
                None => {
                    info!("Connection with ENTG was closed");
                    break;
                }
            }
        }
        // Clean up
        drop(entg2tun_sender);
    };
    Ok(tokio::spawn(async {
        // Stop another when one of then finished
        tokio::select! {
            _ = to_entg => {},
            _ = from_entg  => {}
        };
    }))
}

async fn setup_tun(tun_addr: IpAddr, tun_mask: IpAddr) -> Result<AsyncDevice> {
    info!("Setting up TUN device");

    let mut config = tun::Configuration::default();
    config
        .layer(Layer::L3)
        .address(tun_addr)
        .netmask(tun_mask)
        .up();

    config.platform(|config| {
        config.packet_information(true);
    });

    let dev = tun::create_as_async(&config).context("Failed to create tun device");
    if dev.is_ok() {
        info!(
            "TUN device is ready, address: {} mask: {}",
            tun_addr, tun_mask
        );
    }
    dev
}

async fn exchange_with_tun(
    dev: AsyncDevice,
    tun2entg_sender: Sender<ENPacket>,
    mut entg2tun_receiver: Receiver<ENPacket>,
) -> Result<JoinHandle<()>> {
    let (mut split_sink, mut split_stream) = dev.into_framed().split();

    let to_tun = async move {
        loop {
            match entg2tun_receiver.recv().await {
                Some(packet) => {
                    if let Err(e) = split_sink.send(TunPacket::new(packet.into())).await {
                        error!("Failed to send IP packet to TUN device: {}", e);
                        break;
                    }
                }
                None => {
                    debug!("entg2tun channel closed");
                    break;
                }
            }
        }
        // Clean up
        drop(entg2tun_receiver);
        info!("Close TUN device");
        if let Err(e) = split_sink.close().await {
            error!("Failed to shutdown TUN device: {}", e);
        }
    };

    let from_tun = async move {
        while let Some(frame) = split_stream.next().await {
            match frame {
                Ok(packet) => {
                    debug!("IP Packet from TUN device {:?}", packet);
                    // TODO: fix double copy here
                    if let Err(e) = tun2entg_sender
                        .send(Bytes::from(packet.get_bytes().to_owned()))
                        .await
                    {
                        info!("tun2entg channel closed, close TUN devices: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to receive data from TUN device: {}", e);
                }
            }
        }
        // Clean up
        drop(tun2entg_sender);
    };

    Ok(tokio::spawn(async {
        // Stop another when one of then finished
        tokio::select! {
            _ = to_tun => {},
            _ = from_tun  => {}
        };
    }))
}
