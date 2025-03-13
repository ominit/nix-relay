use std::{process::Stdio, time::Duration};

use futures::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    time::sleep,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const URL: &str = "ws://localhost:4000/worker";
const UPLOAD_URL: &str = "http://localhost:4000";

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
                                    send_derivation(&derivation).await;
                                    ws_stream
                                        .send(Message::text(format!(
                                            "complete true {}",
                                            derivation
                                        )))
                                        .await
                                        .unwrap();
                                    println!("complete true {}", derivation);
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
                                    println!("complete false {}", derivation);
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
    // tokio::fs::write(&derivation, data)
    //     .await
    //     .map_err(|e| e.to_string())?;
    println!("Building derivation: {}", derivation);
    let build_output = print_command(
        Command::new("nix-store")
            .arg("--realize")
            .arg(derivation)
            .arg("-v")
            .stdout(Stdio::piped()),
    )
    .await;
    println!("{:?}", build_output.status);

    if build_output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&build_output.stderr).into_owned())
    }
}

async fn print_command(command: &mut Command) -> std::process::Output {
    let mut child = command.spawn().unwrap();
    let stdout = child.stdout.take().unwrap();

    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await.unwrap() {
        println!("{}", line);
    }

    child.wait_with_output().await.unwrap()
}

async fn send_derivation(derivation: &str) {
    let output = Command::new("nix")
        .arg("copy")
        .arg("--to")
        .arg(UPLOAD_URL)
        .arg(derivation)
        .arg("--refresh")
        .arg("--repair")
        .arg("--derivation")
        .arg("-v")
        .output()
        .await
        .map_err(|e| e.to_string())
        .unwrap();
    println!("{:?}", output);
}
