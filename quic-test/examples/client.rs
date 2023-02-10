use std::net::SocketAddr;

use anyhow::Result;
use s2n_quic::{client::Connect, Client};
use tokio::io;

const CERT: &str = include_str!("../fixtures/cert.pem");

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::builder()
        .with_tls(CERT)?
        .with_io("0.0.0.0:0")?
        .start()
        .map_err(|e| anyhow::anyhow!("Failed to start client. Error: {e}"))?;

    let addr: SocketAddr = "127.0.0.1:4433".parse()?;
    let connect = Connect::new(addr).with_server_name("localhost");
    println!("Connected to {}", addr);

    let mut conn = client.connect(connect).await?;

    conn.keep_alive(true)?;

    let stream = conn.open_bidirectional_stream().await?;

    let (mut rx, mut tx) = stream.split();

    // spawn tokio task to copy server data to stdout
    tokio::spawn(async move {
        let mut stdout = io::stdout();
        io::copy(&mut rx, &mut stdout).await?;
        Ok::<(), anyhow::Error>(())
    });

    // copy stdin to server
    let mut stdin = io::stdin();
    io::copy(&mut stdin, &mut tx).await?;

    Ok(())
}
