use std::{collections::HashMap, process::Output, sync::Arc, thread::JoinHandle, time::Duration};

use anyhow::{Result, bail};
use futures::{SinkExt, StreamExt};
use parking_lot::Mutex;
use tokio::{
    net::TcpStream,
    process::Command,
    sync::mpsc::{self, Receiver, Sender},
    time::sleep,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use tracing_subscriber::fmt::format;

use crate::debug_println;

#[derive(Debug)]
pub struct ServerConnection {
    ws_stream: Option<Arc<Mutex<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
    waiters: Arc<Mutex<HashMap<String, Sender<String>>>>,
}

impl ServerConnection {
    pub fn new() -> Self {
        ServerConnection {
            ws_stream: None,
            waiters: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn connect(&mut self, url: &String) -> Result<()> {
        // TODO make this try a few times
        debug_println!("Connecting to server");
        let test = connect_async(url).await;
        if test.is_ok() {
            self.ws_stream = Some(Arc::new(Mutex::new(test.unwrap().0)));
            {
                let ws_stream = self.ws_stream.as_ref().unwrap().clone();
                let waiters = self.waiters.clone();
                std::thread::spawn(async || Self::websocket_receiver(ws_stream, waiters));
            }
            return Ok(());
        }
        bail!("Failed to connect to server: {:?}", test)
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        self.ws_stream.as_mut().unwrap().lock().close(None).await?;
        debug_println!("Disconnected from server");
        Ok(())
    }

    pub async fn send_build_request(
        &mut self,
        derivation_name: String,
        derivation_raw: Vec<u8>,
    ) -> Result<Receiver<String>> {
        self.ws_stream
            .as_ref()
            .unwrap()
            .lock()
            .send(Message::text(format!(
                "job {} {}",
                derivation_name,
                String::from_utf8_lossy(&derivation_raw)
            )))
            .await?;
        let (sender, receiver) = mpsc::channel::<String>(1);
        self.waiters.lock().insert(derivation_name, sender);
        Ok(receiver)
    }

    async fn websocket_receiver(
        ws_stream: Arc<Mutex<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
        waiters: Arc<Mutex<HashMap<String, Sender<String>>>>,
    ) {
        loop {
            if waiters.lock().is_empty() {
                sleep(Duration::from_secs(1)).await;
                continue;
            }
            let msg = ws_stream.lock().next().await.unwrap().unwrap();
            match msg {
                Message::Text(utf8_bytes) => {}
                _ => todo!(),
            };
            sleep(Duration::from_secs(1)).await;
        }
    }

    pub async fn download_derivation(cache_url: &String, derivation: &String) -> Result<Output> {
        let output = Command::new("nix")
            .arg("copy")
            .arg("--from")
            .arg(cache_url)
            .arg(derivation)
            .arg("--refresh")
            .arg("--repair")
            .arg("--derivation")
            .arg("-v")
            .output()
            .await?;
        Ok(output)
    }

    pub async fn upload_derivation(cache_url: &String, derivation: &String) -> Result<Output> {
        let output = Command::new("nix")
            .arg("copy")
            .arg("--to")
            .arg(cache_url)
            .arg(derivation)
            .arg("--refresh")
            .arg("--repair")
            .arg("--derivation")
            .arg("-v")
            .output()
            .await?;
        Ok(output)
    }
}
