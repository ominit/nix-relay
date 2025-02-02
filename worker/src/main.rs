use futures::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const URL: &str = "ws://localhost:4000/";
const MAX_JOBS: &str = "4";

#[tokio::main]
async fn main() {
    let (mut ws_stream, _) = connect_async(URL.to_string() + "/ws")
        .await
        .expect("failed to connect");

    ws_stream
        .send(Message::text(
            r#"
            {
                "topic": "worker:lobby",
                "event": "phx_join",
                "payload": {},
                "ref": "1"
            }
        "#,
        ))
        .await
        .unwrap();

    loop {
        if let Some(msg) = ws_stream.next().await {
            match msg.unwrap() {
                Message::Text(text) => {
                    if text.contains("new_job") {
                        println!("{:?}", text);
                    }
                }
                _ => {}
            }
        }
    }
}
