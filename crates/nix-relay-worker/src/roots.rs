use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Mutex,
};

use color_eyre::eyre::{Result, WrapErr};
use nix_relay_protocol::BuildId;

pub struct Roots {
    dir: Option<PathBuf>,
    lock: Mutex<()>,
}

impl Roots {
    pub fn new(dir: PathBuf) -> Self {
        let dir = match reset(&dir) {
            Ok(()) => Some(dir),
            Err(error) => {
                tracing::warn!(
                    "not keeping garbage collector roots, so a GC during a build can delete its paths: {error:#}"
                );
                None
            }
        };
        Self {
            dir,
            lock: Mutex::new(()),
        }
    }

    pub fn build_key(build: BuildId) -> String {
        format!("builds/{build}")
    }

    pub fn job_key(drv: &str) -> String {
        let name = drv.rsplit('/').next().unwrap_or(drv);
        format!("jobs/{name}")
    }

    pub fn protect<'a>(&self, key: &str, paths: impl IntoIterator<Item = &'a String>) {
        let Some(dir) = &self.dir else {
            return;
        };
        let _guard = self.lock.lock();
        if let Err(error) = link(&dir.join(key), paths) {
            tracing::warn!(key, "failed to add garbage collector roots: {error:#}");
        }
    }

    pub fn release(&self, key: &str) {
        let Some(dir) = &self.dir else {
            return;
        };
        let _guard = self.lock.lock();
        match std::fs::remove_dir_all(dir.join(key)) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(key, %error, "failed to remove garbage collector roots"),
        }
    }
}

fn reset(dir: &Path) -> Result<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error).wrap_err_with(|| format!("clearing {}", dir.display())),
    }
    std::fs::create_dir_all(dir).wrap_err_with(|| format!("creating {}", dir.display()))
}

fn link<'a>(dir: &Path, paths: impl IntoIterator<Item = &'a String>) -> Result<()> {
    std::fs::create_dir_all(dir).wrap_err_with(|| format!("creating {}", dir.display()))?;
    for path in paths {
        let Some(name) = path.rsplit('/').next().filter(|name| !name.is_empty()) else {
            continue;
        };
        match std::os::unix::fs::symlink(path, dir.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error).wrap_err_with(|| format!("rooting {path}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
