
use anyhow::Result;
use futures::prelude::*;
use kv::{CommandResponse};
use tokio::net::TcpListener;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let addr = "127.0.0.1:9527";
    let listener = TcpListener::bind(addr).await?;
    info!("Start listening on {}", addr);
    loop {
        let (stream, addr) = listener.accept().await?;
        info!("Client {:?} connected", addr);
        tokio::spawn(async move {
            let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
            while let Some(msg) = framed.try_next().await? {
                info!("Got a new command: {:?}", msg);
                // 创建一个 404 response 返回给客户端
                let mut resp = CommandResponse::default();
                resp.status = 404;
                resp.message = "Not found".to_string();
                framed.send(resp.into()).await.unwrap();
            }
            info!("Client {:?} disconnected", addr);
            Ok::<_, anyhow::Error>(())
        });
    }
}