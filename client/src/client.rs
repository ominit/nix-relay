use std::{collections::HashMap, time::Duration};

use anyhow::{Result, bail};
use tokio::{net::TcpStream, process::Command, runtime::Runtime, time::sleep};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::{
    cli::{Cli, DevelopArgs, RebuildArgs, RunArgs},
    config::Config,
    debug_println,
    model::Derivation,
    websocket::Websocket,
};

#[derive(Debug)]
pub struct Client {
    config: Config,
    cli: Cli,
    derivations: HashMap<String, Derivation>,
    server_ws: Websocket,
}

impl Client {
    pub fn new(config: Config, cli: Cli) -> Self {
        debug_println!("config: {:?}", config);
        debug_println!("arguments: {:?}", cli);
        Self {
            config,
            cli,
            derivations: HashMap::new(),
            server_ws: Websocket {},
        }
    }

    pub fn run(self) -> Result<()> {
        match self.cli.command {
            crate::cli::Commands::Develop(ref args) => {
                let args = args.clone();
                self.handle_develop(args)
            }
            crate::cli::Commands::Run(ref args) => {
                let args = args.clone();
                self.handle_run(args)
            }
            crate::cli::Commands::Rebuild(ref args) => {
                let args = args.clone();
                self.handle_rebuild(args)
            }
        }
    }

    fn handle_rebuild(mut self, args: RebuildArgs) -> Result<()> {
        let rebuild_type_arg: String = args.rebuild_type.into();

        debug_println!(
            "Running `nixos-rebuild {} --flake {}`",
            rebuild_type_arg,
            args.flake_ref,
            // self.config.run_args()
        );
        std::process::Command::new("nixos-rebuild")
            .arg(rebuild_type_arg)
            .arg("--flake")
            .arg(args.flake_ref)
            // .args(self.config.rebuild_args()) // Placeholder if config has specific rebuild args
            .status()?;

        Ok(())
    }

    fn handle_run(mut self, args: RunArgs) -> Result<()> {
        debug_println!(
            "Running `nix develop {} {:?}`",
            args.flake_ref,
            self.config.run_args()
        );
        std::process::Command::new("nix")
            .arg("run")
            .arg(args.flake_ref)
            .args(self.config.run_args())
            .status()?;

        Ok(())
    }

    fn handle_develop(mut self, args: DevelopArgs) -> Result<()> {
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
        rt.block_on(self.connect_to_server())?; // TODO make this parallel with getting the derivation
        rt.block_on(self.build(&root_derivation))?;

        // TODO exit server connection
        debug_println!(
            "Running `nix develop {} {:?}`",
            args.flake_ref,
            self.config.develop_args()
        );
        std::process::Command::new("nix")
            .arg("develop")
            .arg(args.flake_ref)
            .args(self.config.develop_args())
            .status()?;

        Ok(())
    }

    async fn connect_to_server(&mut self) -> Result<()> {
        let config = &self.config;
        // let mut ws_stream;
        // loop {
        //     let test = connect_async(config.websocket_url()).await;
        //     if test.is_ok() {
        //         ws_stream = test.unwrap().0;
        //         break;
        //     }
        //     sleep(Duration::from_secs(2)).await;
        // }
        Ok(())
    }

    async fn build(&self, derivation: &String) -> Result<()> {
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
