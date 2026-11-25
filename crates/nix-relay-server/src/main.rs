mod relay;
mod scheduler;

use std::{
    io::IsTerminal,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use axum::{
    Router,
    extract::{State, WebSocketUpgrade},
    response::Response,
    routing::get,
};
use clap::Parser;
use color_eyre::eyre::{Result, WrapErr};
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

use crate::relay::{Event, Relay};

#[derive(Parser)]
#[command(
    version,
    about = "Relay that hands Nix builds from nixr to connected workers"
)]
struct Args {
    #[arg(long, default_value = "[::]:7375")]
    listen: SocketAddr,
    #[arg(long)]
    cache_url: String,
    #[arg(long)]
    cache_store: String,
}

#[derive(Clone)]
struct AppState {
    events: mpsc::Sender<Event>,
    ids: Arc<AtomicU64>,
}

impl AppState {
    fn next_id(&self) -> u64 {
        self.ids.fetch_add(1, Ordering::Relaxed)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
    let args = Args::parse();

    let (events, rx) = mpsc::channel(1024);
    tokio::spawn(Relay::new(args.cache_url, args.cache_store).run(rx));

    let state = AppState {
        events,
        ids: Arc::new(AtomicU64::new(1)),
    };
    let app = Router::new()
        .route("/worker", get(worker))
        .route("/client", get(client))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(args.listen)
        .await
        .wrap_err_with(|| format!("binding {}", args.listen))?;
    tracing::info!(listen = %args.listen, "relay listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn worker(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    let id = state.next_id();
    ws.on_upgrade(move |socket| relay::serve_worker(socket, state.events, id))
}

async fn client(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    let id = state.next_id();
    ws.on_upgrade(move |socket| relay::serve_client(socket, state.events, id))
}
