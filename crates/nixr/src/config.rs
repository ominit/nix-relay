use std::path::PathBuf;

use color_eyre::eyre::{Result, WrapErr, eyre};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub relay: String,
    pub token_file: Option<PathBuf>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .ok_or_else(|| eyre!("neither XDG_CONFIG_HOME nor HOME is set"))?;
        let path = dir.join("nixr").join("config.toml");
        let text = std::fs::read_to_string(&path).wrap_err_with(|| {
            format!(
                "reading {}, which needs at least `relay = \"ws://<host>:7375\"`",
                path.display()
            )
        })?;
        toml::from_str(&text).wrap_err_with(|| format!("parsing {}", path.display()))
    }

    pub fn token(&self) -> Result<Option<String>> {
        self.token_file
            .as_ref()
            .map(|path| {
                std::fs::read_to_string(path)
                    .map(|token| token.trim().to_owned())
                    .wrap_err_with(|| format!("reading {}", path.display()))
            })
            .transpose()
    }
}
