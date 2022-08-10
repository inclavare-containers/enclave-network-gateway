use std::{
    io::{Read, Write},
    os::unix::prelude::AsRawFd,
    sync::Arc,
};

use anyhow::{anyhow, Context, Result};
use clap::{ArgGroup, Parser};
use log::{error, info};
use rats_tls::RatsTls;
// use rats_tls_sys::*;
use tokio::{
    io::DuplexStream,
    net::{TcpListener, TcpStream},
};
use tokio_util::io::SyncIoBridge;

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

    let task1 = get_enta_stream(args.listen_enta);
    tokio::pin!(task1);

    let task2 = async {
        let is_server = args.peer_addr.is_none();
        let tcp_stream = get_entg_stream(args.peer_addr, args.listen_entg).await?;
        upgrade_to_rats_tls(tcp_stream, is_server).await
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

    let (mut enta_r, mut enta_w) = tokio::io::split(enta_stream.unwrap());
    let (mut entg_r, mut entg_w) = tokio::io::split(entg_stream.unwrap());

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

    // Convert to std::net::TcpStream, then set socket to non-block
    let std_tcp_stream = stream.into_std().and_then(|std_tcp_stream| {
        std_tcp_stream
            .set_nonblocking(false)
            .and(Ok(std_tcp_stream))
    })?;

    let rats_tls_session = Arc::new((tls, std_tcp_stream));

    {
        let rats_tls_session = rats_tls_session.clone();
        tokio::task::spawn_blocking(move || {
            rats_tls_session
                .0
                .negotiate(rats_tls_session.1.as_raw_fd())
                .map_err(|err| anyhow!("Failed in rats-tls negotiate: error {}", err))
        })
        .await??;
    }

    let (s1, s2) = tokio::io::duplex(1024);

    let (rh, wh) = tokio::io::split(s1);

    {
        let rats_tls_session = rats_tls_session.clone();
        tokio::task::spawn_blocking(move || {
            let mut rh = SyncIoBridge::new(rh);
            let mut buf = vec![0; 1024];
            while let Ok(r_len) = rh.read(&mut buf) {
                let mut w_off = 0;
                while w_off < r_len {
                    match rats_tls_session.0.transmit(&buf[w_off..r_len]) {
                        Ok(w_len) => w_off += w_len,
                        Err(err) => {
                            error!("Failed in rats-tls teansmit(): error {}", err);
                            return;
                        }
                    };
                }
            }
        });
    }
    {
        tokio::task::spawn_blocking(move || {
            let mut wh = SyncIoBridge::new(wh);
            let mut buf = vec![0; 1024];
            loop {
                match rats_tls_session.0.receive(&mut buf) {
                    Ok(r_len) => {
                        if wh.write_all(&buf[..r_len]).is_err() {
                            return;
                        }
                    }
                    Err(err) => {
                        error!("Failed in rats-tls receive(): error {}", err);
                        return;
                    }
                };
            }
        });
    }

    Ok(s2)
}

async fn get_entg_stream(peer_addr: Option<String>, entg_listen_port: u16) -> Result<TcpStream> {
    Result::<TcpStream>::Ok(match peer_addr {
        Some(peer_addr) => {
            info!("Connect to the peer ENTG: {}", peer_addr);
            let stream = connect_to(peer_addr).await?;
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
