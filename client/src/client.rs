use std::collections::HashMap;

use anyhow::{Result, bail};
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
    derivations: HashMap<String, Derivation>,
}

impl Client {
    pub fn new(config: Config, cli: Cli) -> Self {
        debug_println!("config: {:?}", config);
        debug_println!("arguments: {:?}", cli);
        Self {
            config,
            cli,
            derivations: HashMap::new(),
        }
    }

    pub fn run(self) -> Result<()> {
        match self.cli.command {
            crate::cli::Commands::Run(ref args) => {
                let args = args.clone();
                self.handle_run(args)
            }
        }
    }

    fn handle_run(mut self, args: RunArgs) -> Result<()> {
        let rt = Runtime::new()?;

        let (root_derivation, root_derivation_data) =
            rt.block_on(Self::get_derivation_from_flake(&args.flake_ref))?;
        debug_println!(
            "Root derivation: {:#?}\n{:#?}",
            &root_derivation,
            root_derivation_data
        );
        self.derivations
            .insert(root_derivation.clone(), root_derivation_data);
        rt.block_on(self.connect_to_server())?;
        rt.block_on(self.build(&root_derivation))?;
        Ok(())
    }

    async fn connect_to_server(&mut self) -> Result<()> {
        Ok(())
    }

    async fn build(self, derivation: &String) -> Result<()> {
        debug_println!("Checking derivation: {:?}", derivation);
        let derivation_data = self.derivations.get(derivation).unwrap();
        // check if derivation exists locally, exit out if it does
        let derivation_exists_locally = {
            let output = Command::new("nix-store")
                .arg("--verify-path")
                .arg(derivation_data.outputs.get("out").unwrap().path.clone())
                .output()
                .await?;
            output.status.success()
        };
        debug_println!("Derivation exists locally: {:?}", derivation_exists_locally);
        if derivation_exists_locally {
            return Ok(());
        }
        // check if server has the derivation, exit out if it does
        // check the dependencies of the derivation (run build again), sending any dependencies that exist locally but not on the server
        // send server the derivation file, or the actual binary (nix copy)
        // "build" the derivation (check to make sure it actually exists properly)
        Ok(())
    }

    async fn get_derivation_from_flake(flake_ref: &String) -> Result<(String, Derivation)> {
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

        Self::parse_derivation(drv_show_output.stdout)
    }

    async fn get_derivation(derivation: &String) -> Result<(String, Derivation)> {
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

        Self::parse_derivation(drv_show_output.stdout)
    }

    fn parse_derivation(derivation: Vec<u8>) -> Result<(String, Derivation)> {
        let derivation_data_hashmap = serde_json::from_str::<HashMap<String, Derivation>>(
            &String::from_utf8(derivation.clone())?,
        )?;
        let derivation_name = (*derivation_data_hashmap
            .keys()
            .collect::<Vec<_>>()
            .first()
            .unwrap())
        .clone();

        let mut derivation_data = derivation_data_hashmap
            .get(&derivation_name)
            .unwrap()
            .clone();

        derivation_data.derivation_binary = derivation;

        Ok((derivation_name, derivation_data))
    }
}
