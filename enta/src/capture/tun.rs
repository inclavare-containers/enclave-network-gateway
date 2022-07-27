use std::net::IpAddr;

use anyhow::{Context, Result};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use log::{debug, error, info};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use tun::{AsyncDevice, Layer, TunPacket};

use crate::packet::ENPacket;

pub async fn setup_tun(tun_addr: IpAddr, tun_mask: IpAddr) -> Result<AsyncDevice> {
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

pub async fn exchange_with_tun(
    dev: AsyncDevice,
    outcome_tx: Sender<ENPacket>,
    mut income_rx: Receiver<ENPacket>,
) -> Result<JoinHandle<()>> {
    let (mut split_sink, mut split_stream) = dev.into_framed().split();

    let to_tun = async move {
        loop {
            match income_rx.recv().await {
                Some(packet) => {
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
                    debug!("IP Packet from TUN device {:?}", packet);
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

    Ok(tokio::spawn(async {
        // Stop another when one of then finished
        tokio::select! {
            _ = to_tun => {},
            _ = from_tun  => {}
        };
    }))
}
