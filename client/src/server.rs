use std::{collections::HashMap, process::Output, sync::Arc};

use crate::debug_println;
use anyhow::{Result, bail};
use futures::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use tokio::{
    net::TcpStream,
    process::Command,
    sync::{
        Mutex,
        mpsc::{self, Receiver, Sender},
    },
    task::JoinHandle,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

#[derive(Debug)]
pub struct ServerConnection {
    ws_stream: Option<Arc<Mutex<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>>,
    ws_sink: Option<Arc<Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>>,
    waiters: Arc<Mutex<HashMap<String, Sender<bool>>>>,
    receiver_handle: Option<JoinHandle<()>>,
}

impl ServerConnection {
    pub fn new() -> Self {
        ServerConnection {
            ws_stream: None,
            ws_sink: None,
            waiters: Arc::new(Mutex::new(HashMap::new())),
            receiver_handle: None,
        }
    }

    pub async fn connect(&mut self, url: &String) -> Result<()> {
        // TODO make this try a few times
        debug_println!("Connecting to server at {}...", url);

        if self.ws_sink.is_some() || self.receiver_handle.is_some() {
            debug_println!("Already connected or receiver task active, disconnecting first.");
            if let Err(e) = self.disconnect().await {
                debug_println!("Error during pre-connect disconnect: {:?}", e);
            }
        }

        let test_result = connect_async(url).await;
        if test_result.is_err() {
            bail!(
                "Failed to connect to server {}: {:?}",
                url,
                test_result.err()
            );
        }

        let (sink, stream) = test_result.unwrap().0.split();
        self.ws_stream = Some(Arc::new(Mutex::new(stream)));
        self.ws_sink = Some(Arc::new(Mutex::new(sink)));
        {
            let ws_stream_clone = self.ws_stream.as_ref().unwrap().clone();
            let waiters_clone = self.waiters.clone();
            self.receiver_handle = Some(tokio::task::spawn(async move {
                Self::websocket_receiver(ws_stream_clone, waiters_clone).await;
            }));
        }

        debug_println!("Successfully connected to server and receiver task started.");
        return Ok(());
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        debug_println!("Disconnecting from server...");
        if let Some(sink_arc) = self.ws_sink.take() {
            let mut sink = sink_arc.lock().await;
            if let Err(e) = sink.close().await {
                // Log error, but proceed with other cleanup
                debug_println!("Error closing websocket sink: {:?}", e);
            }
        }
        self.ws_stream = None; // Drop our Arc reference

        if let Some(handle) = self.receiver_handle.take() {
            debug_println!("Waiting for receiver task to shut down...");
            match handle.await {
                Ok(_) => debug_println!("Receiver task shut down gracefully."),
                Err(e) => {
                    // This is a JoinError
                    if e.is_panic() {
                        debug_println!("Receiver task panicked!");
                    } else if e.is_cancelled() {
                        debug_println!("Receiver task was cancelled.");
                    } else {
                        debug_println!("Receiver task completed with an error: {:?}", e);
                    }
                    // Potentially bail or return an error if task failure is critical
                }
            }
        } else {
            debug_println!("No receiver task handle found during disconnect.");
        }

        // The receiver task is now responsible for cleaning up waiters upon its termination.
        // If additional cleanup is needed here (e.g. if task could be aborted), it could be done.
        // For now, we assume graceful shutdown of the task handles its waiters.
        self.waiters.lock().await.clear(); // This might be too aggressive if task manages it.

        debug_println!("Disconnected from server.");
        Ok(())
    }

    pub async fn send_build_request(
        &mut self,
        derivation_name: String,
        derivation_raw: Vec<u8>,
    ) -> Result<Receiver<bool>> {
        self.ws_sink
            .as_ref()
            .unwrap()
            .lock()
            .await
            .send(Message::text(format!(
                "job {} {}",
                derivation_name,
                String::from_utf8_lossy(&derivation_raw)
            )))
            .await?;
        let (sender, receiver) = mpsc::channel::<bool>(32);
        self.waiters.lock().await.insert(derivation_name, sender);
        Ok(receiver)
    }

    async fn websocket_receiver(
        ws_stream: Arc<Mutex<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
        waiters: Arc<Mutex<HashMap<String, Sender<bool>>>>,
    ) {
        loop {
            let msg = ws_stream.lock().await.next().await.unwrap().unwrap();
            match msg {
                Message::Text(utf8_bytes) => {
                    let message = utf8_bytes.to_string();
                    debug_println!("Received message: {:?}", message);
                    let (derivation, success) = message.split_once(" ").unwrap();
                    debug_println!("Received derivation: {:?}", derivation);
                    debug_println!("Received success: {:?}", success);
                    let success = success.parse::<bool>().unwrap();
                    let sender = waiters.lock().await.remove(derivation).unwrap();
                    sender.send(success).await.unwrap();
                }
                msg => todo!("{:?}", msg),
            };
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
