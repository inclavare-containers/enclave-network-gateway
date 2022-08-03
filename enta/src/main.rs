mod capture;
mod packet;

use std::net::IpAddr;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use futures::{SinkExt, StreamExt};
use log::{error, info};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::{
    net::TcpStream,
    sync::mpsc::{Receiver, Sender},
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use packet::ENPacket;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Host address and port of ENTG
    #[clap(long, value_parser, default_value = "127.0.0.1:6980")]
    entg_addr: String,

    /// Set address for tun device
    #[clap(long, value_parser, default_value = "192.168.0.1")]
    tun_addr: IpAddr,

    /// Set network mask for tun device
    #[clap(long, value_parser, default_value = "255.255.255.0")]
    tun_mask: IpAddr,

    ///
    #[clap(long, value_parser)]
    mode: EntaMode,
}

// TODO: Do not distinguish between client and server
#[derive(ValueEnum, Copy, Clone, Debug)]
pub enum EntaMode {
    Client,
    Server,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let args = Args::parse();
    let stream = connect_to_entg(&args.entg_addr).await?;
    let dev = capture::tun::setup_tun(args.tun_addr, args.tun_mask, args.mode).await?;

    let (outcome_tx, outcome_rx) = mpsc::channel(128);
    let (income_tx, income_rx) = mpsc::channel(128);
    let handle1 = exchange_with_entg(stream, income_tx, outcome_rx).await?;
    let handle2 = capture::tun::exchange_with_tun(dev, outcome_tx, income_rx).await?;

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
    income_tx: Sender<ENPacket>,
    mut outcome_rx: Receiver<ENPacket>,
) -> Result<JoinHandle<()>> {
    let (mut split_sink, mut split_stream) =
        Framed::new(stream, LengthDelimitedCodec::new()).split();
    let to_entg = async move {
        loop {
            match outcome_rx.recv().await {
                Some(packet) => {
                    if let Err(e) = split_sink.send(packet.into()).await {
                        error!("Failed to send data to ENTG: {}", e);
                        break;
                    }
                }
                None => {
                    info!("No more packets to send to ENTG, shutdown connection to ENTG");
                    break;
                }
            }
        }
    };

    let from_entg = async move {
        loop {
            match split_stream.next().await {
                Some(Ok(packet)) => {
                    if let Err(e) = income_tx.send(packet.freeze()).await {
                        info!(
                            "All capturers are closed. We will drop the subsequent packets from ENTG: {}",
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
    };
    Ok(tokio::spawn(async {
        // Stop another when one of then finished
        tokio::select! {
            _ = to_entg => {},
            _ = from_entg  => {}
        };
    }))
}
