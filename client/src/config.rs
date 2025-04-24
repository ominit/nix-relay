use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    server_url: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server_url: "".to_string(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config: Self = confy::load("nix-relay", "nixr")?;
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

    // pub async fn read_from_file() -> Self {
    //     #[allow(deprecated)] // windows not supported
    //     let home_dir = std::env::home_dir().unwrap();
    //     toml::from_str(
    //         &tokio::fs::read_to_string(Path::new(&home_dir).join(".config/nix-relay/client.toml"))
    //             .await
    //             .expect("Unable to read ~/.config/nix-relay/client.toml"),
    //     )
    //     .expect("unable to parse config")
    // }

    // pub fn read_from_env() -> Self {
    //     toml::from_str(
    //         &std::env::var("NIX_RELAY_CLIENT")
    //             .expect("environment variable not found: `NIX_RELAY_CLIENT`"),
    //     )
    //     .expect("unable to parse config")
    // }

    // pub fn temp() -> Self {
    //     Config {
    //         server_url: "localhost:4000".to_string(),
    //     }
    // }
}
