use anyhow::Result;
use futures::prelude::*;
use kv::{CommandRequest, CommandResponse};
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let addr = "127.0.0.1:9527";

    // 连接服务器
    let stream = TcpStream::connect(addr).await?;

    let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

    let cmd = CommandRequest::new_hset("table1", "hello", "world1".into());

    framed.send(cmd.into()).await?;

    if let Some(data) = framed.try_next().await? {
        info!("Got reponse {:?}", data);
        let response = CommandResponse::try_from(data.freeze())?;
        println!("Response: {response:?}");
    }

    let cmd = CommandRequest::new_hget("table1", "hello");

    framed.send(cmd.into()).await?;

    if let Some(data) = framed.try_next().await? {
        info!("Got reponse {:?}", data);
        let response = CommandResponse::try_from(data.freeze())?;
        println!("Response: {response:?}");
    }

    Ok(())
}
