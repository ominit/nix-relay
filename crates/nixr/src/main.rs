mod build;
mod config;
mod flake;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;
use nix_relay_protocol::Outcome;

use crate::config::Config;

#[derive(Parser)]
#[command(version, about = "Build Nix flakes on a nix-relay build pool")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Build {
        #[arg(default_value = ".")]
        installable: String,
    },
}

#[tokio::main]
async fn main() -> Result<ExitCode> {
    color_eyre::install()?;
    let cli = Cli::parse();
    match cli.command {
        Command::Build { installable } => {
            let config = Config::load()?;
            let token = config.token()?;
            let request = flake::request(&installable).await?;
            match build::remote(&config.relay, token.as_deref(), request).await? {
                Outcome::Succeeded {
                    outputs,
                    fetch_from,
                } => {
                    build::fetch(&outputs, fetch_from.as_deref()).await?;
                    Ok(ExitCode::SUCCESS)
                }
                Outcome::Failed { error } => {
                    eprintln!("error: {error}");
                    Ok(ExitCode::FAILURE)
                }
            }
        }
    }
}
