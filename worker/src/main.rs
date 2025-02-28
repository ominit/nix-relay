use std::{path::Path, time::Duration};

use futures::{SinkExt, StreamExt};
use tokio::{process::Command, time::sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const URL: &str = "ws://localhost:4000/worker";
const UPLOAD_URL: &str = "http://localhost:4000";
const TEMP_STORE_DIR: &str = "./../temp-store-worker/";

#[tokio::main]
async fn main() {
    loop {
        let mut ws_stream;
        loop {
            let test = connect_async(URL.to_string()).await;
            if test.is_ok() {
                ws_stream = test.unwrap().0;
                break;
            }
            sleep(Duration::from_secs(2)).await;
        }

        let _ = std::fs::create_dir(TEMP_STORE_DIR);

        println!("registering");
        ws_stream.send(Message::text("register")).await.unwrap();

        loop {
            if let Some(msg) = ws_stream.next().await {
                if msg.is_err() {
                    println!("disconnected");
                    break;
                }
                match msg.unwrap() {
                    Message::Text(text) => {
                        if text.contains("request-build") {
                            let (derivation, data) = {
                                let vec = text
                                    .strip_prefix("request-build ")
                                    .unwrap()
                                    .splitn(2, " ")
                                    .collect::<Vec<_>>();
                                (
                                    (*vec.get(0).unwrap()).to_string(),
                                    (*vec.get(1).unwrap()).to_string(),
                                )
                            };
                            println!("Building {:?}", &derivation);

                            let result = build_derivation(&derivation, &data).await;

                            match result {
                                Ok(_) => {
                                    println!("Build successful");
                                    let output = Command::new("nix")
                                        .arg("copy")
                                        .arg("--from")
                                        .arg(TEMP_STORE_DIR)
                                        .arg("--to")
                                        .arg(UPLOAD_URL)
                                        .arg(&derivation)
                                        .arg("-v")
                                        .output()
                                        .await
                                        .map_err(|e| e.to_string())
                                        .unwrap();
                                    println!("{:?}", output);
                                    ws_stream
                                        .send(Message::text(format!(
                                            "complete true {}",
                                            derivation
                                        )))
                                        .await
                                        .unwrap();
                                    println!("Sent Binary");
                                    break;
                                }
                                Err(e) => {
                                    eprintln!("Build failed: {}", e);
                                    ws_stream
                                        .send(Message::text(format!(
                                            "complete false {}",
                                            derivation
                                        )))
                                        .await
                                        .unwrap();
                                }
                            }
                        }
                    }
                    Message::Close(_) => {
                        println!("disconnected");
                        break;
                    }
                    other => {
                        println!("received {:?}", other);
                    }
                }
            }
        }
    }
}

async fn build_derivation(derivation: &str, data: &str) -> Result<(), String> {
    let mut command = Command::new("nix-store");
    let build_output = command
        .arg("--realize")
        .arg(derivation)
        .arg("--store")
        .arg(Path::new(TEMP_STORE_DIR).canonicalize().unwrap())
        .arg("-v")
        .output()
        .await
        .map_err(|e| e.to_string())?;
    println!("{:?}", build_output.status);

    if build_output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&build_output.stderr).into_owned())
    }
}
