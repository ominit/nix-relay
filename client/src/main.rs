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
                        Command::new("nix")
                            .arg("copy")
                            .arg("--from")
                            .arg(DOWNLOAD_URL)
                            .arg(drv_path.replace(".tar.gz", ""))
                            .arg("-v")
                            .output()
                            .await
                            .map_err(|e| e.to_string())
                            .unwrap();
                    }
                    ws_stream.close(None).await.unwrap();
                    std::process::exit(0);
                }
                Message::Binary(_) => {
                    // let output_path =
                    //     std::env::var("out").expect("$out environment variable not set");
                    // // let tmp_path = format!(
                    // //     "/tmp/{}.nar.xz",
                    // //     drv_path
                    // //         .replace("tar.xz.drv", "nar.xz")
                    // //         .replace("/nix/store/", "")
                    // // );
                    // eprintln!("{:?}", output_path);
                    // let mut tmp_file = tokio::fs::File::create(&output_path).await.unwrap();
                    // tokio::io::copy(&mut &bin[..], &mut tmp_file).await.unwrap();
                    // tmp_file.sync_all().await.unwrap();
                    // std::process::exit(0);
                    eprintln!("binary received");
                }
                Message::Ping(_) => {
                    eprintln!("ping received");
                }
                Message::Pong(_) => {
                    eprintln!("pong received");
                }
                Message::Close(_) => {
                    eprintln!("close received");
                    std::process::exit(-1);
                }
                Message::Frame(_) => {
                    eprintln!("frame received");
                }
            }
        }
    }
}
