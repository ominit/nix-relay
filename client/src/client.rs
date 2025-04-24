use std::collections::HashMap;

use anyhow::{Result, bail};
use serde_json::Map;
use tokio::{process::Command, runtime::Runtime};

use crate::{
    cli::{Cli, RunArgs},
    config::Config,
    debug_println,
    model::Derivation,
};

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
        match self.cli.command {
            crate::cli::Commands::Run(ref args) => {
                let args = args.clone();
                self.handle_run(args)
            }
        }
    }

    fn handle_run(self, args: RunArgs) -> Result<()> {
        let rt = Runtime::new()?;
        let root_derivation = rt.block_on(Self::get_derivation_from_flake(&args.flake_ref))?;
        debug_println!("Root derivation: {:#?}", root_derivation);
        rt.block_on(self.build(root_derivation))?;
        Ok(())
    }

    async fn build(self, derivation: HashMap<String, Derivation>) -> Result<()> {
        // check if derivation already created
        // send root derivation to server
        // server sends back what it needs
        // send server the requested derivation file, or the actual binary (nix copy)
        Ok(())
    }

    async fn get_derivation_from_flake(flake_ref: &String) -> Result<HashMap<String, Derivation>> {
        debug_println!("Resolving flake reference: {:?}", flake_ref);

        let drv_show_output = Command::new("nix")
            .arg("derivation")
            .arg("show")
            .arg(&flake_ref)
            .output()
            .await?;

        if !drv_show_output.status.success() {
            bail!(
                "Failed to get derivation for flake '{}':\n{}",
                flake_ref,
                String::from_utf8_lossy(&drv_show_output.stderr)
            )
        }

        debug_println!(
            "derivation: {}",
            String::from_utf8_lossy(&drv_show_output.stdout)
        );

        Self::parse_derivation(&String::from_utf8(drv_show_output.stdout)?)
    }

    async fn get_derivation(derivation: &String) -> Result<HashMap<String, Derivation>> {
        debug_println!("Resolving derivation: {:?}", derivation);

        let drv_show_output = Command::new("nix")
            .arg("derivation")
            .arg("show")
            .arg(&derivation)
            .output()
            .await?;

        if !drv_show_output.status.success() {
            bail!(
                "Failed to get derivation for '{}':\n{}",
                derivation,
                String::from_utf8_lossy(&drv_show_output.stderr)
            )
        }

        debug_println!(
            "derivation: {}",
            String::from_utf8_lossy(&drv_show_output.stdout)
        );

        Self::parse_derivation(&String::from_utf8(drv_show_output.stdout)?)
    }

    fn parse_derivation(derivation: &String) -> Result<HashMap<String, Derivation>> {
        Ok(serde_json::from_str::<HashMap<String, Derivation>>(
            &derivation,
        )?)
    }
}
