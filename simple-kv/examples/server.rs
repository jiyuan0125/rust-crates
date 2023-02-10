
use anyhow::Result;
use futures::prelude::*;
use kv::{CommandResponse, Service, MemTable};
use tokio::net::TcpListener;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let addr = "127.0.0.1:9527";
    let listener = TcpListener::bind(addr).await?;
    info!("Start listening on {}", addr);

    // 初始化 service
    let service: Service = Service::new(MemTable::new());

    loop {
        let (stream, addr) = listener.accept().await?;
        info!("Client {:?} connected", addr);

        // 复制一份 service
        let svc = service.clone();
        tokio::spawn(async move {
            let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
            while let Some(msg) = framed.try_next().await? {
                info!("Got a new command: {:?}", msg);
                // 反序列化出 CommandRequest
                let cmd = msg.freeze().try_into()?;
                // 执行 service, 获得 CommandResponse
                let resp = svc.execute(cmd);
                framed.send(resp.into()).await.unwrap();
            }
            info!("Client {:?} disconnected", addr);
            Ok::<_, anyhow::Error>(())
        });
    }
}