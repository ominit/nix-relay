use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use color_eyre::eyre::{Result, WrapErr, bail};
use futures_util::{SinkExt, StreamExt};
use nix_relay_protocol::{BuildId, RelayMessage, VERSION, WorkerInfo, WorkerMessage, decode_frame};
use tokio::{
    sync::mpsc,
    task::{AbortHandle, JoinSet},
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::{
    jobs::Context,
    nix,
    plan::{Planner, cache_urls},
    roots::Roots,
};

const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

pub struct Settings {
    pub relay: String,
    pub info: WorkerInfo,
    pub gc_roots: PathBuf,
}

pub async fn run(settings: &Settings) -> Result<()> {
    let mut url = settings.relay.trim_end_matches('/').to_owned();
    url.push_str("/worker");
    let (ws, _) = connect_async(url.as_str())
        .await
        .wrap_err_with(|| format!("connecting to {url}"))?;
    let (mut sink, mut stream) = ws.split();

    let register = WorkerMessage::Register {
        version: VERSION,
        info: settings.info.clone(),
    };
    sink.send(Message::Text(serde_json::to_string(&register)?.into()))
        .await?;

    let (cache_url, cache_store) = loop {
        match tokio::time::timeout(IDLE_TIMEOUT, stream.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => match serde_json::from_str(&text)? {
                RelayMessage::Welcome {
                    cache_url,
                    cache_store,
                } => break (cache_url, cache_store),
                RelayMessage::Eval { .. }
                | RelayMessage::UploadDone { .. }
                | RelayMessage::Build { .. }
                | RelayMessage::Protect { .. }
                | RelayMessage::Release { .. } => bail!("relay sent a job before welcoming us"),
            },
            Ok(Some(Ok(
                Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_),
            ))) => {}
            Ok(Some(Ok(Message::Close(_))) | None) => bail!("relay closed the connection"),
            Ok(Some(Err(error))) => return Err(error.into()),
            Err(_) => bail!("relay did not answer the registration"),
        }
    };
    tracing::info!(%url, %cache_url, "connected to relay");

    let substituters = nix::config("substituters").await.unwrap_or_default();
    let (out, mut outgoing) = mpsc::channel::<WorkerMessage>(1024);
    let ctx = Arc::new(Context {
        out,
        planner: Planner::new(cache_urls(&cache_url, &substituters))?,
        cache_url,
        cache_store,
        cache_host: settings.info.cache_host,
        roots: Roots::new(settings.gc_roots.clone()),
    });

    let writer = tokio::spawn(async move {
        while let Some(message) = outgoing.recv().await {
            let text = match serde_json::to_string(&message) {
                Ok(text) => text,
                Err(error) => {
                    tracing::error!(%error, "failed to serialize message");
                    continue;
                }
            };
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let mut tasks = JoinSet::new();
    let mut uploads: HashMap<BuildId, mpsc::Sender<Vec<u8>>> = HashMap::new();
    let mut evals: HashMap<BuildId, AbortHandle> = HashMap::new();
    let (evaluated, mut evaluations) = mpsc::unbounded_channel::<BuildId>();

    let result = loop {
        tokio::select! {
            message = tokio::time::timeout(IDLE_TIMEOUT, stream.next()) => match message {
                Ok(Some(Ok(Message::Text(text)))) => {
                    let message: RelayMessage = match serde_json::from_str(&text) {
                        Ok(message) => message,
                        Err(error) => {
                            tracing::warn!(%error, "ignoring malformed relay message");
                            continue;
                        }
                    };
                    match message {
                        RelayMessage::Welcome { .. } => {}
                        RelayMessage::Eval { build, request } => {
                            tracing::info!(build, ?request, "evaluating");
                            let (tx, rx) = mpsc::channel(16);
                            uploads.insert(build, tx);
                            let ctx = ctx.clone();
                            let evaluated = evaluated.clone();
                            let handle = tasks.spawn(async move {
                                let result = ctx.eval(build, request, rx).await;
                                let _ = ctx.out.send(WorkerMessage::Evaluated { build, result }).await;
                                if !ctx.cache_host {
                                    ctx.roots.release(&Roots::build_key(build));
                                }
                                let _ = evaluated.send(build);
                            });
                            evals.insert(build, handle);
                        }
                        RelayMessage::UploadDone { build } => {
                            uploads.remove(&build);
                        }
                        RelayMessage::Protect { build, paths } => {
                            ctx.roots.protect(&Roots::build_key(build), &paths);
                        }
                        RelayMessage::Release { build } => {
                            if let Some(eval) = evals.remove(&build) {
                                eval.abort();
                            }
                            uploads.remove(&build);
                            ctx.roots.release(&Roots::build_key(build));
                        }
                        RelayMessage::Build { job } => {
                            tracing::info!(drv = %job.drv, "building");
                            let ctx = ctx.clone();
                            tasks.spawn(async move {
                                let drv = job.drv.clone();
                                let result = ctx.build(job).await;
                                tracing::info!(%drv, ?result, "built");
                                let _ = ctx.out.send(WorkerMessage::Built { drv, result }).await;
                            });
                        }
                    }
                }
                Ok(Some(Ok(Message::Binary(frame)))) => {
                    if let Some((build, data)) = decode_frame(&frame)
                        && let Some(tx) = uploads.get(&build)
                        && tx.send(data.to_vec()).await.is_err()
                    {
                        uploads.remove(&build);
                    }
                }
                Ok(Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)))) => {}
                Ok(Some(Ok(Message::Close(_))) | None) => break Ok(()),
                Ok(Some(Err(error))) => break Err(error.into()),
                Err(_) => break Err(color_eyre::eyre::eyre!("relay went quiet")),
            },
            Some(build) = evaluations.recv() => {
                uploads.remove(&build);
                evals.remove(&build);
            }
            Some(joined) = tasks.join_next() => {
                if let Err(error) = joined {
                    tracing::error!(%error, "job task failed");
                }
            }
        }
    };
    tasks.abort_all();
    writer.abort();
    result
}
