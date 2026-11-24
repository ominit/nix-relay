mod drv;
mod jobs;
mod nix;
mod plan;
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
    token_file: Option<PathBuf>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    jobs: Option<usize>,
    #[arg(long)]
    cache_host: bool,
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

    let token = match &args.token_file {
        Some(path) => Some(
            std::fs::read_to_string(path)
                .wrap_err_with(|| format!("reading {}", path.display()))?
                .trim()
                .to_owned(),
        ),
        None => None,
    };
    let settings = Settings {
        relay: args.relay,
        token,
        info: WorkerInfo {
            name: match args.name {
                Some(name) => name,
                None => hostname()?,
            },
            systems: systems().await?,
            features: nix::config("system-features")
                .await?
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            jobs: match args.jobs {
                Some(jobs) => jobs,
                None => default_jobs().await,
            },
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
