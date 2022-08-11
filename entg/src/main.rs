use std::pin::Pin;

use anyhow::{anyhow, Context, Result};
use clap::{ArgGroup, Parser};
use log::info;
use rats_tls::RatsTls;
// use rats_tls_sys::*;
use tokio::{
    io::{AsyncRead, AsyncWrite, DuplexStream},
    net::{TcpListener, TcpStream},
};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(group(
    ArgGroup::new("vers")
        .required(true)
        .args(&["entg-connect", "entg-listen"]),
))]
struct Args {
    /// Host address and port of another ENTG, e.g. "172.17.0.1:6979"
    #[clap(long, value_parser)]
    entg_connect: Option<String>,

    /// Listen port for another ENTG Server
    #[clap(long, value_parser, default_value_t = 6979)]
    entg_listen: u16,

    /// Listen port for ENTA Agent
    #[clap(long, value_parser, default_value_t = 6980)]
    enta_listen: u16,

    /// Establish rats-tls connection with entg
    #[clap(long, value_parser, default_value_t = false)]
    entg_rats_tls: bool,

    /// Establish rats-tls connection with enta
    #[clap(long, value_parser, default_value_t = false)]
    enta_rats_tls: bool,
}

trait AsyncStream: AsyncRead + AsyncWrite {}

impl AsyncStream for DuplexStream {}
impl AsyncStream for TcpStream {}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let args = Args::parse();

    let task1 = async {
        let tcp_stream = get_enta_stream(args.enta_listen).await?;
        let stream: Pin<Box<dyn AsyncStream>> = if args.enta_rats_tls {
            Box::pin(upgrade_to_rats_tls(tcp_stream, true).await?)
        } else {
            Box::pin(tcp_stream)
        };
        Result::<_, anyhow::Error>::Ok(stream)
    };

    tokio::pin!(task1);

    let task2 = async {
        // TODO: replace `is_server` with the options to select attester and verifier
        let is_server = args.entg_connect.is_none();
        let tcp_stream = get_entg_stream(args.entg_connect, args.entg_listen).await?;
        let stream: Pin<Box<dyn AsyncStream>> = if args.entg_rats_tls {
            Box::pin(upgrade_to_rats_tls(tcp_stream, is_server).await?)
        } else {
            Box::pin(tcp_stream)
        };
        Result::<_, anyhow::Error>::Ok(stream)
    };
    tokio::pin!(task2);

    let mut enta_stream = None;
    let mut entg_stream = None;

    while enta_stream.is_none() || entg_stream.is_none() {
        tokio::select! {
            v1 = (&mut task1), if enta_stream.is_none() => {
                enta_stream = Some(v1?);
            },
            v2 = (&mut task2), if entg_stream.is_none() => {
                entg_stream = Some(v2?);
            },
        }
    }

    let enta_stream = enta_stream.unwrap();
    let entg_stream = entg_stream.unwrap();

    let (mut enta_r, mut enta_w) = tokio::io::split(enta_stream);
    let (mut entg_r, mut entg_w) = tokio::io::split(entg_stream);

    info!("Start forwarding");

    tokio::select!(
        r = tokio::io::copy(&mut enta_r, &mut entg_w) => {info!("Connection from ENTA is closed"); r?},
        r = tokio::io::copy(&mut entg_r, &mut enta_w) => {info!("Connection from ENTG is closed"); r?},
    );

    info!("Shutdown ENTG Server");
    Ok(())
}

async fn get_enta_stream(enta_listen_port: u16) -> Result<TcpStream> {
    info!("Waiting for ENTA on port {}", enta_listen_port);
    let stream = listen_on_port(enta_listen_port).await?;
    info!("Connection received from ENTA: {}", stream.peer_addr()?);
    Result::<TcpStream>::Ok(stream)
}

async fn upgrade_to_rats_tls(stream: TcpStream, server: bool) -> Result<DuplexStream> {
    let tls = if server {
        RatsTls::new(
            server,
            0,
            Some("openssl"),
            Some("openssl"),
            Some("sgx_ecdsa"),
            Some("nullverifier"),
            false,
        )
    } else {
        RatsTls::new(
            server,
            0,
            Some("openssl"),
            Some("openssl"),
            Some("nullattester"),
            Some("sgx_ecdsa"),
            false,
        )
    }
    .map_err(|err| anyhow!("Failed to init rats-tls: error {:#x}", err))?;

    tls.negotiate_async(stream)
        .await
        .context("Failed in rats-tls negotiation")
}

async fn get_entg_stream(entg_connect: Option<String>, entg_listen_port: u16) -> Result<TcpStream> {
    Result::<TcpStream>::Ok(match entg_connect {
        Some(entg_connect) => {
            info!("Connect to the peer ENTG: {}", entg_connect);
            let stream = connect_to(entg_connect).await?;
            info!("Connection established with ENTG: {}", stream.peer_addr()?);
            stream
        }
        _ => {
            info!("Waiting for ENTG on port {}", entg_listen_port);
            let stream = listen_on_port(entg_listen_port).await?;
            info!("Connection received from ENTG: {}", stream.peer_addr()?);
            stream
        }
    })
}

async fn listen_on_port(port: u16) -> Result<TcpStream> {
    let listener = TcpListener::bind(("0.0.0.0", port))
        .await
        .with_context(|| format!("Failed to bind port {}", port))?;

    let (stream, _) = listener.accept().await?;
    return Ok(stream);
}

async fn connect_to(addr: String) -> Result<TcpStream> {
    Ok(TcpStream::connect(addr).await?)
}
