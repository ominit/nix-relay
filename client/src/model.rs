use anyhow::Result;
use derivative::Derivative;
use serde::Deserialize;
use std::collections::HashMap;
use tokio::process::Command;

use crate::{debug_println, server::ServerConnection};

#[derive(Deserialize, Derivative, Clone)]
#[derivative(Debug)]
#[serde(rename_all = "camelCase")]
pub struct Derivation {
    // pub args: Vec<String>,
    // pub builder: String,
    // pub env: HashMap<String, String>,
    pub input_drvs: HashMap<String, InputDrvDetails>,
    // pub input_srcs: Vec<String>,
    pub name: String,
    pub outputs: HashMap<String, OutputDetails>,
    pub system: System,
    #[serde(default)]
    #[derivative(Debug = "ignore")]
    pub derivation_binary: Vec<u8>,
    #[serde(default)]
    pub is_local: bool,
    #[serde(default)]
    pub is_on_server: bool,
}

impl Derivation {
    pub fn get_dependencies(&self) -> Vec<String> {
        self.input_drvs.keys().cloned().collect()
    }

    pub async fn derivation_exists_locally(&self) -> Result<bool> {
        let derivation_exists_locally = {
            let output = Command::new("nix-store")
                .arg("--verify-path")
                .arg(self.outputs.get("out").unwrap().path.clone())
                .output()
                .await?;
            output.status.success()
        };
        debug_println!("Derivation exists locally: {:?}", derivation_exists_locally);
        if derivation_exists_locally {
            return Ok(true);
        }
        Ok(false)
    }

    pub async fn download_derivation(
        &self,
        cache_url: &String,
        derivation_name: &String,
    ) -> Result<bool> {
        let output = ServerConnection::download_derivation(cache_url, derivation_name).await?;
        debug_println!("Derivation exists in server: {:?}", output);
        if output.status.success() {
            return Ok(true);
        }
        Ok(false)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub enum System {
    #[serde(rename = "x86_64-linux")]
    X86_64Linux,
    #[serde(rename = "builtin")]
    Builtin,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InputDrvDetails {
    pub dynamic_outputs: HashMap<String, serde_json::Value>,
    pub outputs: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct OutputDetails {
    pub path: String,
}
