use std::{collections::HashMap, time::Duration};

use axum::extract::ws::{Message, WebSocket};
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use nix_relay_protocol::{
    BuildId, ClientEvent, ClientMessage, Outcome, RelayMessage, VERSION, WorkerInfo, WorkerMessage,
    encode_frame,
};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::mpsc;

use crate::scheduler::{Action, Scheduler, WorkerId};

const PING_INTERVAL: Duration = Duration::from_secs(20);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

pub enum Outgoing {
    Text(String),
    Binary(Vec<u8>),
}

pub enum Event {
    WorkerJoined {
        id: WorkerId,
        info: WorkerInfo,
        tx: mpsc::Sender<Outgoing>,
    },
    Worker {
        id: WorkerId,
        message: WorkerMessage,
    },
    WorkerLeft {
        id: WorkerId,
    },
    ClientJoined {
        build: BuildId,
        tx: mpsc::Sender<Outgoing>,
    },
    Client {
        build: BuildId,
        message: ClientMessage,
    },
    ClientData {
        build: BuildId,
        data: Vec<u8>,
    },
    ClientLeft {
        build: BuildId,
    },
}

pub struct Relay {
    cache_url: String,
    cache_store: String,
    scheduler: Scheduler,
    workers: HashMap<WorkerId, (String, mpsc::Sender<Outgoing>)>,
    clients: HashMap<BuildId, mpsc::Sender<Outgoing>>,
}

impl Relay {
    pub fn new(cache_url: String, cache_store: String) -> Self {
        Self {
            scheduler: Scheduler::new(cache_url.clone()),
            cache_url,
            cache_store,
            workers: HashMap::new(),
            clients: HashMap::new(),
        }
    }

    pub async fn run(mut self, mut events: mpsc::Receiver<Event>) {
        while let Some(event) = events.recv().await {
            let actions = self.handle(event).await;
            self.apply(actions).await;
        }
    }

    async fn handle(&mut self, event: Event) -> Vec<Action> {
        match event {
            Event::WorkerJoined { id, info, tx } => {
                tracing::info!(worker = %info.name, jobs = info.jobs, cache_host = info.cache_host, "worker joined");
                let welcome = RelayMessage::Welcome {
                    cache_url: self.cache_url.clone(),
                    cache_store: self.cache_store.clone(),
                };
                send(&tx, &welcome).await;
                self.workers.insert(id, (info.name.clone(), tx));
                self.scheduler.add_worker(id, info)
            }
            Event::WorkerLeft { id } => {
                if let Some((name, _)) = self.workers.remove(&id) {
                    tracing::info!(worker = %name, "worker left");
                }
                self.scheduler.remove_worker(id)
            }
            Event::Worker { id, message } => self.worker_message(id, message),
            Event::ClientJoined { build, tx } => {
                self.clients.insert(build, tx);
                Vec::new()
            }
            Event::ClientLeft { build } => {
                self.clients.remove(&build);
                self.scheduler.client_gone(build)
            }
            Event::Client { build, message } => match message {
                ClientMessage::Submit { version, request } => {
                    if version == VERSION {
                        tracing::info!(build, ?request, "build submitted");
                        self.scheduler.submit(build, request)
                    } else {
                        let error = format!(
                            "nixr speaks protocol version {version}, but the relay speaks {VERSION}"
                        );
                        vec![Action::Client(
                            build,
                            ClientEvent::Finished {
                                outcome: Outcome::Failed { error },
                            },
                        )]
                    }
                }
                ClientMessage::UploadDone => self.scheduler.upload_done(build),
            },
            Event::ClientData { build, data } => {
                if let Some(worker) = self.scheduler.eval_worker(build)
                    && let Some((_, tx)) = self.workers.get(&worker)
                {
                    let frame = encode_frame(build, &data);
                    if tx.send(Outgoing::Binary(frame)).await.is_err() {
                        tracing::debug!(build, "eval worker went away during upload");
                    }
                }
                Vec::new()
            }
        }
    }

    fn worker_message(&mut self, id: WorkerId, message: WorkerMessage) -> Vec<Action> {
        match message {
            WorkerMessage::Register { .. } => Vec::new(),
            WorkerMessage::Upload { build, paths } => {
                self.scheduler.upload_requested(id, build, paths)
            }
            WorkerMessage::EvalLog { build, line } => self.scheduler.eval_log(id, build, line),
            WorkerMessage::Evaluated { build, result } => {
                self.scheduler.evaluated(id, build, result)
            }
            WorkerMessage::BuildLog { drv, line } => self.scheduler.build_log(&drv, &line),
            WorkerMessage::Built { drv, result } => self.scheduler.built(id, &drv, result),
        }
    }

    async fn apply(&self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Worker(id, message) => {
                    if let Some((_, tx)) = self.workers.get(&id) {
                        send(tx, &message).await;
                    }
                }
                Action::Client(build, event) => {
                    if let ClientEvent::Finished { outcome } = &event {
                        match outcome {
                            Outcome::Succeeded { .. } => tracing::info!(build, "build succeeded"),
                            Outcome::Failed { error } => {
                                tracing::info!(build, %error, "build failed");
                            }
                        }
                    }
                    if let Some(tx) = self.clients.get(&build) {
                        send(tx, &event).await;
                    }
                }
            }
        }
    }
}

async fn send(tx: &mpsc::Sender<Outgoing>, message: &(impl Serialize + Sync)) {
    match serde_json::to_string(message) {
        Ok(text) => {
            if tx.send(Outgoing::Text(text)).await.is_err() {
                tracing::debug!("connection closed before a message could be sent");
            }
        }
        Err(error) => tracing::error!(%error, "failed to serialize message"),
    }
}

pub async fn serve_worker(socket: WebSocket, events: mpsc::Sender<Event>, id: WorkerId) {
    let (sink, mut stream) = socket.split();
    let info = match next_text::<WorkerMessage>(&mut stream).await {
        Some(WorkerMessage::Register { version, info }) if version == VERSION => info,
        Some(WorkerMessage::Register { version, info }) => {
            tracing::warn!(worker = %info.name, version, "rejected worker with a different protocol version");
            return;
        }
        Some(
            WorkerMessage::Upload { .. }
            | WorkerMessage::EvalLog { .. }
            | WorkerMessage::Evaluated { .. }
            | WorkerMessage::BuildLog { .. }
            | WorkerMessage::Built { .. },
        )
        | None => {
            tracing::warn!("worker connection closed before it registered");
            return;
        }
    };
    let (tx, rx) = mpsc::channel(256);
    let writer = tokio::spawn(write(sink, rx));
    if events
        .send(Event::WorkerJoined { id, info, tx })
        .await
        .is_err()
    {
        return;
    }
    loop {
        match read(&mut stream).await {
            Read::Text(text) => match serde_json::from_str(&text) {
                Ok(message) => {
                    if events.send(Event::Worker { id, message }).await.is_err() {
                        break;
                    }
                }
                Err(error) => tracing::warn!(%error, "ignoring malformed worker message"),
            },
            Read::Binary(_) | Read::Other => {}
            Read::Closed => break,
        }
    }
    writer.abort();
    let _ = events.send(Event::WorkerLeft { id }).await;
}

pub async fn serve_client(socket: WebSocket, events: mpsc::Sender<Event>, build: BuildId) {
    let (sink, mut stream) = socket.split();
    let (tx, rx) = mpsc::channel(256);
    let writer = tokio::spawn(write(sink, rx));
    if events
        .send(Event::ClientJoined { build, tx })
        .await
        .is_err()
    {
        return;
    }
    loop {
        let event = match read(&mut stream).await {
            Read::Text(text) => match serde_json::from_str(&text) {
                Ok(message) => Event::Client { build, message },
                Err(error) => {
                    tracing::warn!(%error, "ignoring malformed client message");
                    continue;
                }
            },
            Read::Binary(data) => Event::ClientData { build, data },
            Read::Other => continue,
            Read::Closed => break,
        };
        if events.send(event).await.is_err() {
            break;
        }
    }
    writer.abort();
    let _ = events.send(Event::ClientLeft { build }).await;
}

enum Read {
    Text(String),
    Binary(Vec<u8>),
    Other,
    Closed,
}

async fn read(stream: &mut SplitStream<WebSocket>) -> Read {
    match tokio::time::timeout(IDLE_TIMEOUT, stream.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => Read::Text(text.as_str().to_owned()),
        Ok(Some(Ok(Message::Binary(data)))) => Read::Binary(data.to_vec()),
        Ok(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => Read::Other,
        Ok(Some(Ok(Message::Close(_)) | Err(_)) | None) | Err(_) => Read::Closed,
    }
}

async fn next_text<T: DeserializeOwned>(stream: &mut SplitStream<WebSocket>) -> Option<T> {
    loop {
        match read(stream).await {
            Read::Text(text) => return serde_json::from_str(&text).ok(),
            Read::Binary(_) | Read::Other => {}
            Read::Closed => return None,
        }
    }
}

async fn write(mut sink: SplitSink<WebSocket, Message>, mut rx: mpsc::Receiver<Outgoing>) {
    let mut ping = tokio::time::interval(PING_INTERVAL);
    loop {
        let message = tokio::select! {
            outgoing = rx.recv() => match outgoing {
                Some(Outgoing::Text(text)) => Message::Text(text.into()),
                Some(Outgoing::Binary(data)) => Message::Binary(data.into()),
                None => break,
            },
            _ = ping.tick() => Message::Ping(Vec::new().into()),
        };
        if sink.send(message).await.is_err() {
            break;
        }
    }
    let _ = sink.close().await;
}
