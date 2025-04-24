use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "nixr", version, about = "A client for nix-relay")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    // /// Enters a development environment (like nix develop)
    // Develop {
    //     #[arg(default_value = ".")]
    //     flake_ref: String,
    // },
    /// Runs an executable from a flake output (like nix run)
    Run(RunArgs),
}

#[derive(Args, Debug, Clone)]
pub struct RunArgs {
    #[arg(default_value = ".")]
    pub flake_ref: String,
}
