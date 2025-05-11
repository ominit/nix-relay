use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "nixr", version, about = "A client for nix-relay")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Runs an executable from a flake output (like nix run)
    Run(RunArgs),
    /// Enters a development environment (like nix develop)
    Develop(DevelopArgs),
    /// Rebuilds the system configuration (like nixos-rebuild)
    Rebuild(RebuildArgs),
}

#[derive(Args, Debug, Clone)]
pub struct DevelopArgs {
    #[arg(default_value = ".")]
    pub flake_ref: String,
}

#[derive(Args, Debug, Clone)]
pub struct RunArgs {
    #[arg(default_value = ".")]
    pub flake_ref: String,
}

#[derive(Args, Debug, Clone)]
pub struct RebuildArgs {
    #[arg(value_enum)]
    pub rebuild_type: RebuildType,
    #[arg(default_value = ".")]
    pub flake_ref: String,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum RebuildType {
    Switch,
    Boot,
    Test,
    Build,
    #[value(name = "dry-build")]
    DryBuild,
    #[value(name = "dry-activate")]
    DryActivate,
    Edit,
    Repl,
    #[value(name = "build-vm")]
    BuildVM,
    #[value(name = "build-vm-with-bootloader")]
    BuildVMWithBootloader,
    #[value(name = "build-image")]
    BuildImage,
    #[value(name = "list-generations")]
    ListGenerations,
}

impl Into<String> for RebuildType {
    fn into(self) -> String {
        match self {
            RebuildType::Switch => "switch".to_string(),
            RebuildType::Boot => "boot".to_string(),
            RebuildType::Test => "test".to_string(),
            RebuildType::Build => "build".to_string(),
            RebuildType::DryBuild => "dry-build".to_string(),
            RebuildType::DryActivate => "dry-activate".to_string(),
            RebuildType::Edit => "edit".to_string(),
            RebuildType::Repl => "repl".to_string(),
            RebuildType::BuildVM => "build-vm".to_string(),
            RebuildType::BuildVMWithBootloader => "build-vm-with-bootloader".to_string(),
            RebuildType::BuildImage => "build-image".to_string(),
            RebuildType::ListGenerations => "list-generations".to_string(),
        }
    }
}
