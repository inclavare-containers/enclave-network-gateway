use anyhow::{Context, Result};
use clap::{ArgEnum, Parser};
use futures::{SinkExt, StreamExt};
use tun::{AsyncDevice, Layer};

#[tokio::main]
async fn main() -> Result<()> {
    setup_tun().await?;
    Ok(())
}

async fn setup_tun() -> Result<()> {
    let mut config = tun::Configuration::default();
    config
        .layer(Layer::L3)
        .address((192, 168, 100, 1))
        .netmask((255, 255, 255, 0))
        .up();

    config.platform(|config| {
        config.packet_information(true);
    });

    let dev: AsyncDevice = tun::create_as_async(&config).context("Failed to create tun device")?;
    let mut framed = dev.into_framed();

    while let Some(packet) = framed.next().await {
        match packet {
            Ok(pkt) => println!("{:?}", pkt.get_bytes()),
            Err(err) => panic!("Error: {:?}", err),
        }
    }
    Ok(())
}
