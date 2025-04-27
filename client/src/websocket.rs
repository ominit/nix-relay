use std::time::Duration;

use anyhow::{Error, Result, bail};
use tokio::{net::TcpStream, time::sleep};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::debug_println;

#[derive(Debug)]
pub struct Websocket {
    ws_stream: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
}

impl Websocket {
    pub fn new() -> Self {
        Websocket { ws_stream: None }
    }

    pub async fn connect(&mut self, url: &String) -> Result<()> {
        debug_println!("Connecting to server");
        let test = connect_async(url).await;
        if test.is_ok() {
            self.ws_stream = Some(test.unwrap().0);
            return Ok(());
        }
        bail!("Failed to connect to server: {:?}", test)
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        self.ws_stream.as_mut().unwrap().close(None).await?;
        debug_println!("Disconnected from server");
        Ok(())
    }
}
