use anyhow::{Context, Result};
use clap::{ArgGroup, Parser};
use log::info;
// use rats_tls_sys::*;
use tokio::net::{TcpListener, TcpStream};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(group(
    ArgGroup::new("vers")
        .required(true)
        .args(&["peer-addr", "listen-entg"]),
))]
struct Args {
    /// Host address and port of another ENTG, e.g. "172.17.0.1:6979"
    #[clap(long, value_parser)]
    peer_addr: Option<String>,

    /// Listen port for another ENTG Server
    #[clap(long, value_parser, default_value_t = 6979)]
    listen_entg: u16,

    /// Listen port for ENTA Agent
    #[clap(long, value_parser, default_value_t = 6980)]
    listen_enta: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let args = Args::parse();

    let get_enta_stream = (|| async {
        info!("Waiting for ENTA on port {}", args.listen_enta);
        let stream = listen_on_port(args.listen_enta).await?;
        info!("Connection received from ENTA: {}", stream.peer_addr()?);
        Result::<TcpStream>::Ok(stream)
    })();
    tokio::pin!(get_enta_stream);

    let get_entg_stream = (|| async {
        Result::<TcpStream>::Ok(match args.peer_addr {
            Some(peer_addr) => {
                info!("Connect to the peer ENTG: {}", peer_addr);
                let stream = connect_to(peer_addr).await?;
                info!("Connection established with ENTG: {}", stream.peer_addr()?);
                stream
            }
            _ => {
                info!("Waiting for ENTG on port {}", args.listen_entg);
                let stream = listen_on_port(args.listen_entg).await?;
                info!("Connection received from ENTG: {}", stream.peer_addr()?);
                stream
            }
        })
    })();
    tokio::pin!(get_entg_stream);

    let mut enta_stream = None;
    let mut entg_stream = None;

    while enta_stream.is_none() || entg_stream.is_none() {
        tokio::select! {
            v1 = (&mut get_enta_stream), if enta_stream.is_none() => {
                enta_stream = Some(v1?);
            },
            v2 = (&mut get_entg_stream), if entg_stream.is_none() => {
                entg_stream = Some(v2?);
            },
        }
    }

    let (mut enta_r, mut enta_w) = enta_stream.unwrap().into_split();
    let (mut entg_r, mut entg_w) = entg_stream.unwrap().into_split();

    info!("Start forwarding");

    tokio::select!(
        r = tokio::io::copy(&mut enta_r, &mut entg_w) => {info!("Connection from ENTA is closed"); r?},
        r = tokio::io::copy(&mut entg_r, &mut enta_w) => {info!("Connection from ENTG is closed"); r?},
    );

    info!("Shutdown ENTG Server");
    Ok(())
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
