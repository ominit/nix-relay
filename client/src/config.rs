use std::path::Path;

use anyhow::{Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::debug_println;

#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    #[serde(default)]
    server_url: String,
    #[serde(default)]
    develop_args: Vec<String>,
    #[serde(default)]
    run_args: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server_url: "".to_string(),
            develop_args: vec![],
            run_args: vec![],
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        #[allow(deprecated)] // windows not supported
        let home_dir = std::env::home_dir().unwrap();
        let config: Config = toml::from_str(&std::fs::read_to_string(
            Path::new(&home_dir).join(".config/nix-relay/nixr.toml"),
        )?)?;
        if config.server_url.is_empty() {
            bail!(
                "`server_url` is undefined in the configuration. Please set it in ~/.config/nix-relay/nixr.toml."
            );
        }
        Ok(config)
    }

    pub fn websocket_url(&self) -> String {
        format!("ws://{}/client", self.server_url)
    }

    pub fn cache_url(&self) -> String {
        format!("http://{}", self.server_url)
    }

    pub fn develop_args(&self) -> Vec<String> {
        Self::expand_env_vars(&self.develop_args)
    }

    pub fn run_args(&self) -> Vec<String> {
        Self::expand_env_vars(&self.run_args)
    }

    fn expand_env_vars(vec: &Vec<String>) -> Vec<String> {
        let regex = Regex::new(r"\$(\w+)").unwrap();
        let mut result = vec![];

        for arg in vec {
            let new = regex.replace_all(arg, |caps: &regex::Captures| {
                debug_println!("Expanding env var: {:?}", &caps[1]);
                std::env::var(&caps[1]).expect("Environment variable not found")
            });
            result.push(new.to_string());
        }
        result
    }
}
