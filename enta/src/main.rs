mod capture;
mod packet;

use std::net::IpAddr;
use std::pin::Pin;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use futures::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use rats_tls::RatsTls;
use tokio::io::{AsyncRead, AsyncWrite, DuplexStream};
use tokio::sync::mpsc;
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
    entg_connect: String,

    /// Set address for tun device
    #[clap(long, value_parser, default_value = "192.168.0.1")]
    tun_addr: IpAddr,

    /// Set network mask for tun device
    #[clap(long, value_parser, default_value = "255.255.255.0")]
    tun_mask: IpAddr,

    /// Establish rats-tls connection with entg
    #[clap(long, value_parser, default_value_t = false)]
    entg_rats_tls: bool,

    /// The dport of the packet that needs to be captured. This option is set on the client side.
    #[clap(long, value_parser)]
    capture: Option<u16>, // TODO: capture more than one dport

    /// The dport of the packet to be replayed, corresponding to the capture. This option is set on the server side.
    #[clap(long, value_parser)]
    replay: Option<u16>,
}

trait AsyncStream: AsyncRead + AsyncWrite {}

impl AsyncStream for DuplexStream {}
impl AsyncStream for TcpStream {}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let args = Args::parse();
    let capture = args.capture;
    let replay = args.replay;

    let result = run(args).await;

    // Clean up before program exit
    if let Err(err) = capture::tun::clean_up(capture, replay).await {
        warn!("Failed to clean up: {}", err);
    }
    result
}

async fn run(args: Args) -> Result<()> {
    let stream = connect_to_entg(&args.entg_connect, args.entg_rats_tls).await?;
    let dev =
        capture::tun::setup_tun(args.tun_addr, args.tun_mask, args.capture, args.replay).await?;

    let (outcome_tx, outcome_rx) = mpsc::channel(128);
    let (income_tx, income_rx) = mpsc::channel(128);
    let task1 = exchange_with_entg(stream, income_tx, outcome_rx);
    let task2 = capture::tun::exchange_with_tun(dev, outcome_tx, income_rx);

    let handle = async { tokio::join!(task1, task2) };
    tokio::select! {
        (first, second) = handle => { first.and(second) }
        _ = tokio::signal::ctrl_c() => { Ok(()) }
    }
}

async fn connect_to_entg(
    entg_connect: &str,
    entg_rats_tls: bool,
) -> Result<Pin<Box<dyn AsyncStream>>> {
    info!("Connecting to ENTG");
    let stream = TcpStream::connect(entg_connect)
        .await
        .with_context(|| format!("Falied to connect to ENTG: {}", entg_connect))?;
    info!(
        "Connection with ENTG is established, peer address: {}",
        stream.peer_addr()?
    );
    if entg_rats_tls {
        let stream = upgrade_to_rats_tls(stream).await?;
        info!("Rats-tls channel with ENTG is established");
        Ok(Box::pin(stream))
    } else {
        Ok(Box::pin(stream))
    }
}

async fn upgrade_to_rats_tls(stream: TcpStream) -> Result<DuplexStream> {
    let tls = RatsTls::new(
        false,
        0,
        Some("openssl"),
        Some("openssl"),
        Some("nullattester"),
        Some("sgx_ecdsa"),
        false,
    )
    .map_err(|err| anyhow!("Failed to init rats-tls: error {:#x}", err))?;

    tls.negotiate_async(stream)
        .await
        .context("Failed in rats-tls negotiation")
}

async fn exchange_with_entg<T>(
    stream: T,
    income_tx: Sender<ENPacket>,
    mut outcome_rx: Receiver<ENPacket>,
) -> Result<()>
where
    T: AsyncRead + AsyncWrite + 'static,
{
    let (mut split_sink, mut split_stream) =
        Framed::new(stream, LengthDelimitedCodec::new()).split();
    let to_entg = async move {
        loop {
            match outcome_rx.recv().await {
                Some(packet) => {
                    debug!("=> entg: {} bytes packet", packet.len());
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
                    debug!("<= entg: {} bytes packet", packet.len());
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
    // Stop another when one of then finished
    tokio::select! {
        _ = to_entg => {},
        _ = from_entg  => {}
    }
    Ok(())
}
