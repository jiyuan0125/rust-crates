use anyhow::{anyhow, Result};
use futures::prelude::*;
use s2n_quic::Server;

const CERT: &str = include_str!("../fixtures/cert.pem");
const KEY: &str = include_str!("../fixtures/key.pem");

#[tokio::main]
async fn main() -> Result<()> {
    let addr = "127.0.0.1:4433";

    let mut server = Server::builder()
        .with_tls((CERT, KEY))?
        .with_io(addr)?
        .start()
        .map_err(|e| anyhow!("Failed to start server. Error: {e}"))?;

    println!("Listening on {}", addr);

    while let Some(mut conn) = server.accept().await {
        println!("Accepted connection from {}", conn.remote_addr()?);
        while let Some(mut stream) = conn.accept_bidirectional_stream().await? {
            println!(
                "Accepted stream from {}",
                stream.connection().remote_addr()?
            );

            tokio::spawn(async move {
                while let Some(data) = stream.try_next().await? {
                    println!("Received data: {:?}", data);
                    stream.send(data).await?;
                }
                Ok::<(), anyhow::Error>(())
            });
        }
    }

    Ok(())
}
