use anyhow::{Context, Result};
use clap::{ArgEnum, Parser};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::join;
use tokio::net::{TcpListener, TcpStream};

/// Simple TCP echo client & server
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// What mode to run the program in
    #[clap(arg_enum, value_parser)]
    mode: Mode,

    /// Host address of Server
    #[clap(short, long, value_parser, default_value_t = From::from("127.0.0.1"))]
    host: String,

    /// Ports to connect or listen to
    #[clap(short, long, value_parser, default_value_t = 8080)]
    port: u16,
}

#[derive(Debug, Copy, Clone, ArgEnum)]
enum Mode {
    /// Run as an echo client
    Client,
    /// Run as an echo server
    Server,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    match args.mode {
        Mode::Client => run_echo_client(args).await,
        Mode::Server => run_echo_server(args).await,
    }
}

async fn run_echo_client(args: Args) -> Result<()> {
    let mut stream = TcpStream::connect((args.host, args.port)).await?;
    println!("Connected to {}", stream.peer_addr()?);

    let (mut read_half, mut write_half) = stream.split();

    // Prepare data
    let data = "Hello world".as_bytes();

    // Send all data to server
    let write_data = async {
        let ret = write_half
            .write_all(data)
            .await
            .context("Failed to send data to server");
        write_half
            .shutdown()
            .await
            .context("Failed to shutdown connection")
            .and(ret)
    };

    // Read all data
    let read_data = async {
        let mut buf = vec![0; data.len() + 1]; // We use larger buffer to detect cases where more data is received
        let mut recv_size = 0;
        while recv_size < buf.len() {
            match read_half.read(&mut buf[recv_size..]).await {
                // socket closed
                Ok(n) if n == 0 => break,
                Ok(n) => recv_size += n,
                Err(e) => return Err(e).context("Failed to receive data from server"),
            }
        }
        Ok((buf, recv_size))
    };

    // Waiting for send and receive to complete
    let result = join!(write_data, read_data);
    result.0?;
    let (buf, recv_size) = result.1?;

    // Verify
    println!(
        "Sent {} bytes, received {} bytes\tcheck size: {}\tcheck content: {}",
        data.len(),
        recv_size,
        if data.len() == recv_size {
            "PASS"
        } else {
            "FAILED"
        },
        if data == &buf[..data.len()] {
            "PASS"
        } else {
            "FAILED"
        },
    );
    Ok(())
}

async fn run_echo_server(args: Args) -> Result<()> {
    let listener = TcpListener::bind((args.host, args.port)).await?;
    println!("Listening on {}", listener.local_addr()?);
    loop {
        let (mut stream, _) = listener.accept().await?;
        println!("Inbound connection from {}", stream.peer_addr()?);

        tokio::spawn(async move {
            let mut buf = [0; 1024];
            let ret = loop {
                let n = match stream.read(&mut buf).await {
                    // socket closed
                    Ok(n) if n == 0 => break Ok(()),
                    Ok(n) => n,
                    Err(e) => {
                        break Err(e).context("Failed to receive data from client");
                    }
                };

                // Write the data back
                if let Err(e) = stream.write_all(&buf[0..n]).await {
                    break Err(e).context("Failed to send data to client");
                }
            };
            println!("Connection from {} closed", stream.peer_addr().unwrap());
            ret
        });
    }
}
