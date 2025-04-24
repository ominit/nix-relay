use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Derivation {
    pub args: Vec<String>,
    pub builder: String,
    pub env: HashMap<String, String>,
    pub input_drvs: HashMap<String, InputDrvDetails>,
    pub input_srcs: Vec<String>,
    pub name: String,
    pub outputs: HashMap<String, OutputDetails>,
    pub system: System,
}

#[derive(Deserialize, Debug, Clone)]
pub enum System {
    #[serde(rename = "x86_64-linux")]
    X86_64Linux,
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
