mod cli;
mod client;
mod config;
mod macros;
mod model;
mod server;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use client::Client;
use config::Config;
use std::env;

fn main() -> Result<()> {
    human_panic::setup_panic!(
        human_panic::Metadata::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
            .homepage("github.com/ominit/nix-relay")
    );

    let cli = Cli::parse();

    let config = Config::load()?;

    let client = Client::new(config, cli);

    client.run()
}
