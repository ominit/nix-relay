use std::{collections::HashMap, pin::Pin, sync::Arc};

use anyhow::{Result, bail};
use tokio::{process::Command, runtime::Runtime, sync::Mutex, task::JoinHandle};

use crate::{
    cli::{Cli, DevelopArgs, RebuildArgs, RunArgs},
    config::Config,
    debug_println,
    model::Derivation,
    server::ServerConnection,
};

#[derive(Debug)]
pub struct Client {
    config: Config,
    cli: Cli,
    derivations: Arc<Mutex<HashMap<String, Derivation>>>,
    server: Arc<Mutex<ServerConnection>>,
}

impl Client {
    pub fn new(config: Config, cli: Cli) -> Self {
        debug_println!("config: {:?}", config);
        debug_println!("arguments: {:?}", cli);
        Self {
            config,
            cli,
            derivations: Arc::new(Mutex::new(HashMap::new())),
            server: Arc::new(Mutex::new(ServerConnection::new())),
        }
    }

    pub fn run(self) -> Result<()> {
        let rt = Runtime::new()?;
        match self.cli.command {
            crate::cli::Commands::Develop(ref args) => {
                let args = args.clone();
                rt.block_on(self.handle_develop(args))
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

    async fn handle_develop(mut self, args: DevelopArgs) -> Result<()> {
        let (root_derivation, root_derivation_data) =
            Self::get_derivation_from_flake(&args.flake_ref).await?;
        debug_println!(
            "Root derivation: {:#?}\n{:#?}",
            &root_derivation,
            root_derivation_data
        );
        if root_derivation_data.derivation_exists_locally().await? {
            return Ok(());
        }
        self.derivations
            .lock()
            .await
            .insert(root_derivation.clone(), root_derivation_data);
        self.connect_to_server().await?;
        Self::build(
            self.derivations,
            self.config.clone(),
            root_derivation,
            self.server.clone(),
        )
        .await?;
        self.server.lock().await.disconnect().await?;

        debug_println!(
            "Running `nix develop {} -j 0 {:?}`",
            args.flake_ref,
            self.config.develop_args()
        );
        Command::new("nix")
            .arg("develop")
            .arg(args.flake_ref)
            .arg("-j")
            .arg("0")
            .args(self.config.develop_args())
            .status()
            .await?;

        Ok(())
    }

    async fn connect_to_server(&mut self) -> Result<()> {
        self.server
            .lock()
            .await
            .connect(&self.config.websocket_url())
            .await?;
        Ok(())
    }

    async fn build(
        derivations: Arc<Mutex<HashMap<String, Derivation>>>,
        config: Config,
        derivation: String,
        server: Arc<Mutex<ServerConnection>>,
    ) -> Result<()> {
        debug_println!("Checking derivation: {:?}", derivation);
        let derivation_data = derivations.lock().await.get(&derivation).unwrap().clone();

        // check if derivation exists locally, exit out if it does
        if derivation_data.derivation_exists_locally().await? {
            derivations
                .lock()
                .await
                .get_mut(&derivation)
                .unwrap()
                .is_local = true;
            if ServerConnection::upload_derivation(&config.cache_url(), &derivation)
                .await?
                .status
                .success()
            {
                derivations
                    .lock()
                    .await
                    .get_mut(&derivation)
                    .unwrap()
                    .is_on_server = true;
            }
            return Ok(());
        }

        // check if server has the derivation, exit out if it does
        if derivation_data
            .download_derivation(&config.cache_url(), &derivation)
            .await?
        {
            derivations
                .lock()
                .await
                .get_mut(&derivation)
                .unwrap()
                .is_on_server = true;
            derivations
                .lock()
                .await
                .get_mut(&derivation)
                .unwrap()
                .is_local = true;
            return Ok(());
        }

        // check the dependencies of the derivation (run build again), sending any dependencies that exist locally but not on the server
        let dependencies = derivation_data.get_dependencies();
        let mut tasks: Vec<JoinHandle<Result<()>>> = vec![];
        for dependency in dependencies {
            let (dependency_derivation, dependency_derivation_data) =
                Self::get_derivation(&dependency).await?;
            if derivations
                .lock()
                .await
                .contains_key(&dependency_derivation)
            {
                continue;
            }
            derivations
                .lock()
                .await
                .insert(dependency_derivation, dependency_derivation_data);
            {
                let dependency = dependency.clone();
                let derivations = derivations.clone();
                let config = config.clone();
                let server = server.clone();
                let task: JoinHandle<Result<()>> = tokio::spawn(async move {
                    Self::build_boxed(derivations, config, dependency, server).await
                });
                tasks.push(task);
            }
        }
        for task in tasks {
            task.await??;
        }

        // send server the derivation file
        let mut receiver = server
            .lock()
            .await
            .send_build_request(
                derivation.clone(),
                derivation_data.derivation_binary.clone(),
            )
            .await?;

        // wait for server to finish
        let msg = receiver.recv().await.unwrap();
        if !msg {
            debug_println!("server failed to build derivation {}", derivation);
            Self::build_derivation(&derivation).await?;
            ServerConnection::upload_derivation(&config.cache_url(), &derivation).await?;
        } else {
            if !ServerConnection::download_derivation(&config.cache_url(), &derivation)
                .await?
                .status
                .success()
            {
                debug_println!("derivation failed to download: {}", derivation);
            }
        }

        Ok(())
    }

    fn build_boxed(
        derivations: Arc<Mutex<HashMap<String, Derivation>>>,
        config: Config,
        derivation: String,
        server: Arc<Mutex<ServerConnection>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        Box::pin(Self::build(derivations, config, derivation, server))
    }

    async fn build_derivation(derivation: &String) -> Result<()> {
        let output = Command::new("nix-store")
            .arg("--realize")
            .arg(derivation)
            .status()
            .await?;
        debug_println!("build output: {:?}", output);
        Ok(())
    }

    async fn get_derivation_from_flake(flake_ref: &String) -> Result<(String, Derivation)> {
        // debug_println!("Resolving flake reference: {:?}", flake_ref);

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
        // debug_println!("Resolving derivation: {:?}", derivation);

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

        // debug_println!(
        //     "derivation: {}",
        //     String::from_utf8_lossy(&drv_show_output.stdout)
        // );

        Self::parse_derivation(drv_show_output.stdout)
    }

    fn parse_derivation(derivation_raw: Vec<u8>) -> Result<(String, Derivation)> {
        let derivation_data_hashmap = serde_json::from_str::<HashMap<String, Derivation>>(
            &String::from_utf8(derivation_raw.clone())?,
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

        derivation_data.derivation_binary = derivation_raw;

        Ok((derivation_name, derivation_data))
    }
}
