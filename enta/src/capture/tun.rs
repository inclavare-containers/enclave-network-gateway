use std::net::IpAddr;

use anyhow::{ensure, Context, Result};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use log::{debug, error, info};
use tokio::process::Command;
use tokio::sync::mpsc::{Receiver, Sender};
use tun::{AsyncDevice, Layer, TunPacket};

use crate::packet::ENPacket;
use crate::EntaMode;

pub async fn setup_tun(tun_addr: IpAddr, tun_mask: IpAddr, mode: EntaMode) -> Result<AsyncDevice> {
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

    // Setup iptables rules
    setup_netfilter(mode)
        .await
        .context("Failed to setup netfilter")?;
    dev
}

async fn setup_netfilter(mode: EntaMode) -> Result<()> {
    let mut cmd = Command::new("/bin/sh");
    match mode {
        EntaMode::Client => {
            cmd.args([
                "-e",
                "-c",
                "iptables -t nat -A OUTPUT -p tcp --dport 7 -j DNAT --to-destination 192.168.0.254:6978 ; \
                ip route add default via 192.168.0.1 table 8 ; \
                ip rule add dport 7 table 8 ; \
                ip route flush cache",
            ]);
        }
        EntaMode::Server => {
            cmd.args([
                "-e",
                "-c",
                "iptables -t nat -A PREROUTING -p tcp --dport 6978 -j REDIRECT --to-port 7",
            ]);
        }
    };

    let output = cmd.output().await?;
    ensure!(
        output.status.success(),
        "cmd failed: '{:?}' \nstatus: {:?}\nstderr: {}",
        cmd,
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

pub async fn exchange_with_tun(
    dev: AsyncDevice,
    outcome_tx: Sender<ENPacket>,
    mut income_rx: Receiver<ENPacket>,
) -> Result<()> {
    let (mut split_sink, mut split_stream) = dev.into_framed().split();

    let to_tun = async move {
        loop {
            match income_rx.recv().await {
                Some(packet) => {
                    debug!("=> tun: {} bytes packet", packet.len());
                    if let Err(e) = split_sink.send(TunPacket::new(packet.into())).await {
                        error!("Failed to send IP packet to TUN device: {}", e);
                        break;
                    }
                }
                None => {
                    debug!("Income channel closed, close TUN device now");
                    break;
                }
            }
        }
    };

    let from_tun = async move {
        while let Some(frame) = split_stream.next().await {
            match frame {
                Ok(packet) => {
                    debug!("<= tun: {} bytes packet", packet.get_bytes().len());
                    // TODO: fix double copy here
                    if let Err(e) = outcome_tx
                        .send(Bytes::from(packet.get_bytes().to_owned()))
                        .await
                    {
                        debug!("Outcome channel closed, close TUN device now: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to receive data from TUN device: {}", e);
                }
            }
        }
    };

    // Stop another when one of then finished
    tokio::select! {
        _ = to_tun => {},
        _ = from_tun  => {}
    };
    Ok(())
}
