mod config;

use clap::Parser;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::{env, path::Path};
use tokio::process::Command;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Deserialize)]
struct Config {
    server_url: String,
}

impl Config {
    pub fn websocket_url(&self) -> String {
        format!("ws://{}/client", self.server_url)
    }

    pub fn cache_url(&self) -> String {
        format!("http://{}", self.server_url)
    }

    // pub async fn read_from_file() -> Self {
    //     #[allow(deprecated)] // windows not supported
    //     let home_dir = std::env::home_dir().unwrap();
    //     toml::from_str(
    //         &tokio::fs::read_to_string(Path::new(&home_dir).join(".config/nix-relay/client.toml"))
    //             .await
    //             .expect("Unable to read ~/.config/nix-relay/client.toml"),
    //     )
    //     .expect("unable to parse config")
    // }

    // pub fn read_from_env() -> Self {
    //     toml::from_str(
    //         &std::env::var("NIX_RELAY_CLIENT")
    //             .expect("environment variable not found: `NIX_RELAY_CLIENT`"),
    //     )
    //     .expect("unable to parse config")
    // }

    pub fn temp() -> Self {
        Config {
            server_url: "localhost:4000".to_string(),
        }
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Not enough arguments: {:?}", args);
        return;
    }
    // eprintln!("arguments: {:?}", args);

    let config = Config::temp();

    let drv_path = &args[1];
    let derivation_file = Command::new("nix")
        .arg("derivation")
        .arg("show")
        .arg(drv_path)
        .arg("-r")
        .output()
        .await
        .unwrap()
        .stdout;

    let (mut ws_stream, _) = connect_async(config.websocket_url())
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
                            .arg(config.cache_url())
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
