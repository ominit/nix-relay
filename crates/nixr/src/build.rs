use std::process::Stdio;

use color_eyre::eyre::{Result, WrapErr, bail};
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use nix_relay_protocol::{
    BuildRequest, ClientEvent, ClientMessage, Outcome, VERSION, store_path_name,
};
use tokio::{io::AsyncReadExt, process::Command};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, Message},
};

const CHUNK: usize = 256 * 1024;

pub async fn remote(relay: &str, request: BuildRequest) -> Result<Outcome> {
    let mut url = relay.trim_end_matches('/').to_owned();
    url.push_str("/client");
    let (ws, _) = connect_async(url.as_str())
        .await
        .wrap_err_with(|| format!("connecting to {url}"))?;
    let (mut sink, mut stream) = ws.split();
    send(
        &mut sink,
        &ClientMessage::Submit {
            version: VERSION,
            request,
        },
    )
    .await?;

    let mut total = 0;
    let mut built = 0;
    while let Some(message) = stream.next().await {
        let text = match message? {
            Message::Text(text) => text,
            Message::Close(_) => break,
            Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {
                continue;
            }
        };
        match serde_json::from_str::<ClientEvent>(&text)? {
            ClientEvent::Evaluating { worker } => eprintln!("evaluating on {worker}"),
            ClientEvent::Upload { paths } => {
                eprintln!("uploading {} source paths", paths.len());
                upload(&mut sink, &paths).await?;
                send(&mut sink, &ClientMessage::UploadDone).await?;
            }
            ClientEvent::Planned { derivations } => {
                total = derivations;
                match derivations {
                    0 => {}
                    1 => eprintln!("building 1 derivation"),
                    n => eprintln!("building {n} derivations"),
                }
            }
            ClientEvent::Started { drv, worker } => {
                eprintln!("building {} on {worker}", store_path_name(&drv));
            }
            ClientEvent::Log { line, .. } => eprintln!("{line}"),
            ClientEvent::Built { drv } => {
                built += 1;
                eprintln!("[{built}/{total}] built {}", store_path_name(&drv));
            }
            ClientEvent::Finished { outcome } => {
                if let Outcome::Succeeded {
                    outputs,
                    fetch_from,
                } = &outcome
                {
                    keep_alive(&mut stream, fetch(outputs, fetch_from.as_deref())).await?;
                }
                let _ = sink.close().await;
                return Ok(outcome);
            }
        }
    }
    bail!("the relay closed the connection before the build finished")
}

async fn keep_alive<S, F>(stream: &mut S, work: F) -> Result<()>
where
    S: Stream<Item = Result<Message, tungstenite::Error>> + Unpin + Send,
    F: Future<Output = Result<()>> + Send,
{
    tokio::pin!(work);
    let mut open = true;
    loop {
        tokio::select! {
            result = &mut work => return result,
            message = stream.next(), if open => open = matches!(message, Some(Ok(_))),
        }
    }
}

async fn fetch(outputs: &[String], fetch_from: Option<&str>) -> Result<()> {
    if let Some(cache) = fetch_from {
        let mut copy = Command::new("nix");
        copy.args([
            "copy",
            "--from",
            cache,
            "--option",
            "narinfo-cache-negative-ttl",
            "0",
        ])
        .args(outputs);
        run(copy)
            .await
            .wrap_err_with(|| format!("copying the outputs from {cache}"))?;
    }
    let mut link = Command::new("nix");
    link.args(["build", "--out-link", "result"]).args(outputs);
    run(link).await
}

async fn upload<S>(sink: &mut S, paths: &[String]) -> Result<()>
where
    S: Sink<Message, Error = tungstenite::Error> + Unpin + Send,
{
    let mut child = Command::new("nix-store")
        .arg("--export")
        .args(paths)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .wrap_err("starting nix-store --export")?;
    if let Some(mut stdout) = child.stdout.take() {
        let mut buf = vec![0; CHUNK];
        loop {
            let read = stdout.read(&mut buf).await?;
            let Some(chunk) = buf.get(..read) else {
                break;
            };
            if chunk.is_empty() {
                break;
            }
            sink.send(Message::Binary(chunk.to_vec().into())).await?;
        }
    }
    let status = child.wait().await?;
    if !status.success() {
        bail!("nix-store --export failed with {status}");
    }
    Ok(())
}

async fn send<S>(sink: &mut S, message: &ClientMessage) -> Result<()>
where
    S: Sink<Message, Error = tungstenite::Error> + Unpin + Send,
{
    sink.send(Message::Text(serde_json::to_string(message)?.into()))
        .await?;
    Ok(())
}

async fn run(mut command: Command) -> Result<()> {
    let status = command.stdin(Stdio::null()).status().await?;
    if !status.success() {
        bail!("{:?} failed with {status}", command.as_std());
    }
    Ok(())
}
