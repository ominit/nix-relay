mod drv;
mod jobs;
mod nix;
mod plan;
mod roots;
mod session;

use std::{io::IsTerminal, path::PathBuf, time::Duration};

use clap::Parser;
use color_eyre::eyre::{Result, WrapErr};
use nix_relay_protocol::WorkerInfo;
use tracing_subscriber::EnvFilter;

use crate::session::Settings;

#[derive(Parser)]
#[command(
    version,
    about = "Worker that builds Nix derivations handed out by a nix-relay server"
)]
struct Args {
    #[arg(long)]
    relay: String,
    #[arg(long)]
    cache_host: bool,
    #[arg(long)]
    gc_roots: Option<PathBuf>,
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

    let name = hostname()?;
    let settings = Settings {
        relay: args.relay,
        gc_roots: args
            .gc_roots
            .unwrap_or_else(|| PathBuf::from("/nix/var/nix/gcroots/nix-relay").join(&name)),
        info: WorkerInfo {
            name,
            systems: systems().await?,
            features: nix::config("system-features")
                .await?
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            jobs: default_jobs().await,
            cache_host: args.cache_host,
        },
    };

    let mut backoff = Duration::from_secs(1);
    loop {
        let started = tokio::time::Instant::now();
        match session::run(&settings).await {
            Ok(()) => tracing::warn!("relay closed the connection"),
            Err(error) => tracing::warn!("{error:#}"),
        }
        if started.elapsed() > Duration::from_secs(60) {
            backoff = Duration::from_secs(1);
        }
        tracing::info!(seconds = backoff.as_secs(), "reconnecting");
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

fn hostname() -> Result<String> {
    Ok(std::fs::read_to_string("/proc/sys/kernel/hostname")
        .wrap_err("reading the hostname")?
        .trim()
        .to_owned())
}

async fn systems() -> Result<Vec<String>> {
    let mut systems = vec![nix::config("system").await?];
    for extra in nix::config("extra-platforms").await?.split_whitespace() {
        if !systems.iter().any(|s| s == extra) {
            systems.push(extra.to_owned());
        }
    }
    Ok(systems)
}

async fn default_jobs() -> usize {
    match nix::config("max-jobs")
        .await
        .map(|jobs| jobs.parse::<usize>())
    {
        Ok(Ok(jobs)) if jobs > 0 => jobs,
        Ok(Ok(_) | Err(_)) | Err(_) => std::thread::available_parallelism().map_or(1, usize::from),
    }
}
