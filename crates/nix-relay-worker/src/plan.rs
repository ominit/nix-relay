use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use color_eyre::eyre::{Result, eyre};
use futures_util::{StreamExt, stream::FuturesUnordered};
use nix_relay_protocol::{Plan, PlannedDerivation};
use tokio::sync::Semaphore;

use crate::drv::Derivation;

const CONCURRENT_LOOKUPS: usize = 64;

pub struct Planner {
    caches: Vec<String>,
    http: reqwest::Client,
    lookups: Arc<Semaphore>,
}

impl Planner {
    pub fn new(caches: Vec<String>) -> Result<Self> {
        Ok(Self {
            caches,
            http: reqwest::Client::builder().build()?,
            lookups: Arc::new(Semaphore::new(CONCURRENT_LOOKUPS)),
        })
    }

    pub async fn plan(&self, roots: Vec<(String, BTreeSet<String>)>) -> Result<Plan> {
        let mut drvs: HashMap<String, Derivation> = HashMap::new();
        let mut needed: HashMap<String, BTreeSet<String>> = HashMap::new();
        let mut in_plan: BTreeSet<String> = BTreeSet::new();
        let mut checks = FuturesUnordered::new();
        let mut outputs = Vec::new();
        let mut queue = roots;

        for (drv, names) in &queue {
            let derivation = Derivation::read(drv).await?;
            for name in names {
                let path = derivation
                    .outputs
                    .get(name)
                    .ok_or_else(|| eyre!("{drv} has no output {name}"))?;
                outputs.push(path.clone());
            }
            drvs.insert(drv.clone(), derivation);
        }

        loop {
            while let Some((drv, names)) = queue.pop() {
                let known = needed.entry(drv.clone()).or_default();
                let new: BTreeSet<String> = names.difference(known).cloned().collect();
                if new.is_empty() {
                    continue;
                }
                known.extend(new.iter().cloned());
                if in_plan.contains(&drv) {
                    continue;
                }
                if !drvs.contains_key(&drv) {
                    let derivation = Derivation::read(&drv).await?;
                    drvs.insert(drv.clone(), derivation);
                }
                let Some(derivation) = drvs.get(&drv) else {
                    continue;
                };
                let paths: Option<Vec<String>> = new
                    .iter()
                    .map(|name| {
                        derivation
                            .outputs
                            .get(name)
                            .filter(|path| !path.is_empty())
                            .cloned()
                    })
                    .collect();
                checks.push(async move {
                    let available = match paths {
                        Some(paths) => self.all_available(&paths).await,
                        None => false,
                    };
                    (drv, available)
                });
            }
            let Some((drv, available)) = checks.next().await else {
                break;
            };
            if !available
                && in_plan.insert(drv.clone())
                && let Some(derivation) = drvs.get(&drv)
            {
                queue.extend(
                    derivation
                        .input_drvs
                        .iter()
                        .map(|(input, names)| (input.clone(), names.clone())),
                );
            }
        }
        drop(checks);

        let derivations = in_plan
            .iter()
            .filter_map(|drv| {
                let derivation = drvs.get(drv)?;
                Some(planned(drv, derivation, &in_plan, &drvs))
            })
            .collect();
        Ok(Plan {
            outputs,
            derivations,
        })
    }

    async fn all_available(&self, paths: &[String]) -> bool {
        for path in paths {
            if !self.available(path).await {
                return false;
            }
        }
        true
    }

    async fn available(&self, path: &str) -> bool {
        let Some(hash) = path
            .strip_prefix("/nix/store/")
            .and_then(|base| base.get(..32))
        else {
            return false;
        };
        for cache in &self.caches {
            let Ok(_permit) = self.lookups.acquire().await else {
                return false;
            };
            match self
                .http
                .head(format!("{cache}/{hash}.narinfo"))
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => return true,
                Ok(_) => {}
                Err(error) => tracing::debug!(%cache, %error, "narinfo lookup failed"),
            }
        }
        false
    }
}

fn planned(
    drv: &str,
    derivation: &Derivation,
    in_plan: &BTreeSet<String>,
    drvs: &HashMap<String, Derivation>,
) -> PlannedDerivation {
    let mut inputs = Vec::new();
    let mut fetch = Vec::new();
    for (input, names) in &derivation.input_drvs {
        if !in_plan.contains(input) {
            continue;
        }
        inputs.push(input.clone());
        if let Some(dep) = drvs.get(input) {
            fetch.extend(
                names
                    .iter()
                    .filter_map(|name| dep.outputs.get(name).cloned()),
            );
        }
    }
    PlannedDerivation {
        drv: drv.to_owned(),
        system: derivation.system.clone(),
        features: derivation.required_features(),
        prefer_local: derivation.prefer_local_build(),
        outputs: derivation
            .outputs
            .values()
            .filter(|path| !path.is_empty())
            .cloned()
            .collect(),
        inputs,
        fetch,
    }
}

pub fn cache_urls(cache_url: &str, substituters: &str) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    for url in std::iter::once(cache_url).chain(substituters.split_whitespace()) {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            continue;
        }
        let url = url.split('?').next().unwrap_or(url).trim_end_matches('/');
        if !urls.iter().any(|known| known == url) {
            urls.push(url.to_owned());
        }
    }
    urls
}

pub fn roots(json: &str) -> Result<Vec<(String, BTreeSet<String>)>> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Built {
        drv_path: String,
        outputs: BTreeMap<String, String>,
    }
    let built: Vec<Built> = serde_json::from_str(json)?;
    Ok(built
        .into_iter()
        .map(|b| (b.drv_path, b.outputs.into_keys().collect()))
        .collect())
}

#[cfg(test)]
mod tests;
