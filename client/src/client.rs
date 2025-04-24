use anyhow::Result;

use crate::{cli::Cli, config::Config, debug_println};

#[derive(Debug)]
pub struct Client {
    config: Config,
    cli: Cli,
}

impl Client {
    pub fn new(config: Config, cli: Cli) -> Self {
        debug_println!("config: {:?}", config);
        debug_println!("arguments: {:?}", cli);
        Self { config, cli }
    }

    pub fn run(self) -> Result<()> {
        Ok(())
    }
}
