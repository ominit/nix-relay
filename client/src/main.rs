use futures::{SinkExt, StreamExt};
use std::env;
use tokio::process::Command;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const URL: &str = "ws://localhost:4000/client";
const DOWNLOAD_URL: &str = "http://localhost:4000";

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Not enough arguments: {:?}", args);
        return;
    }
    // eprintln!("arguments: {:?}", args);

    let drv_path = &args[1];
    let derivation_file = Command::new("cat")
        .arg(drv_path)
        .output()
        .await
        .unwrap()
        .stdout;
    // eprintln!("{:?}", String::from_utf8_lossy(&derivation_file));

    let (mut ws_stream, _) = connect_async(URL.to_string())
        .await
        .expect("failed to connect");
    eprintln!("Connected to server");

    ws_stream
        .send(Message::text(format!(
            "job {} {}",
            drv_path,
            String::from_utf8_lossy(&derivation_file)
        )))
        .await
        .unwrap();
    eprintln!("Sent job");

    loop {
        if let Some(msg) = ws_stream.next().await {
            match msg.unwrap() {
                Message::Text(text) => {
                    eprintln!("received message: {:?}", text);
                    if text == "true" {
                        let result = Command::new("nix")
                            .arg("copy")
                            .arg("--from")
                            .arg(DOWNLOAD_URL)
                            .arg(drv_path)
                            .arg("--refresh")
                            .arg("-v")
                            .output()
                            .await
                            .map_err(|e| e.to_string())
                            .unwrap();
                        if result.status.success() {
                            eprintln!("Successfully copied build result from server");
                            // Exit with success - Nix will recognize the build as done
                            std::process::exit(0);
                        } else {
                            eprintln!(
                                "Failed to copy build result: {}",
                                String::from_utf8_lossy(&result.stderr)
                            );
                            // Exit with failure - Nix will attempt to build locally
                            std::process::exit(1);
                        }
                    }
                    ws_stream.close(None).await.unwrap();
                }
                _ => {}
            }
        }
    }
}
