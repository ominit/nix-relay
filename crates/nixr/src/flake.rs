use std::process::Stdio;

use color_eyre::eyre::{Result, WrapErr, bail, eyre};
use nix_relay_protocol::{BuildRequest, nix_string};
use serde_json::Value;
use tokio::process::Command;

pub async fn request(installable: &str) -> Result<BuildRequest> {
    let (flake, fragment) = installable.split_once('#').unwrap_or((installable, ""));
    if fragment.contains('^') {
        bail!("nixr can't select outputs with ^ yet");
    }

    let metadata = nix_json(&["flake", "metadata", "--json", flake]).await?;
    let archive = nix_json(&["flake", "archive", "--json", flake]).await?;
    let url = metadata
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| eyre!("nix flake metadata did not report a locked URL"))?;
    let subdir = metadata
        .pointer("/locked/dir")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let source = archive
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| eyre!("nix flake archive did not report a source path"))?
        .to_owned();
    let mut sources = Vec::new();
    collect_sources(&archive, &mut sources);

    let expr = format!(
        "removeAttrs (builtins.getFlake {}).sourceInfo [ \"outPath\" ]",
        nix_string(url)
    );
    let Value::Object(source_info) =
        nix_json(&["eval", "--json", "--impure", "--expr", &expr]).await?
    else {
        bail!("the flake's sourceInfo is not an attribute set");
    };

    let system = String::from_utf8(nix(&["config", "show", "system"]).await?)
        .wrap_err("nix config show printed a non-UTF-8 system")?;

    Ok(BuildRequest::Flake {
        source,
        source_info,
        subdir,
        attributes: attributes(fragment, system.trim())?,
        sources,
    })
}

pub fn attributes(fragment: &str, system: &str) -> Result<Vec<Vec<String>>> {
    let paths = if let Some(exact) = fragment.strip_prefix('.') {
        vec![exact.to_owned()]
    } else {
        let wanted = if fragment.is_empty() {
            vec![
                format!("packages.{system}.default"),
                format!("defaultPackage.{system}"),
            ]
        } else {
            vec![fragment.to_owned()]
        };
        let first = wanted.first().cloned().unwrap_or_default();
        [
            format!("packages.{system}.{first}"),
            format!("legacyPackages.{system}.{first}"),
        ]
        .into_iter()
        .chain(wanted)
        .collect()
    };
    paths.iter().map(|path| attr_path(path)).collect()
}

fn attr_path(path: &str) -> Result<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        match c {
            '.' => parts.push(std::mem::take(&mut current)),
            '"' => loop {
                match chars.next() {
                    Some('"') => break,
                    Some(c) => current.push(c),
                    None => bail!("missing closing quote in attribute path '{path}'"),
                }
            },
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    Ok(parts)
}

fn collect_sources(node: &Value, sources: &mut Vec<String>) {
    if let Some(path) = node.get("path").and_then(Value::as_str)
        && !sources.iter().any(|known| known == path)
    {
        sources.push(path.to_owned());
    }
    if let Some(inputs) = node.get("inputs").and_then(Value::as_object) {
        for input in inputs.values() {
            collect_sources(input, sources);
        }
    }
}

async fn nix_json(args: &[&str]) -> Result<Value> {
    serde_json::from_slice(&nix(args).await?)
        .wrap_err_with(|| format!("parsing nix {} output", args.join(" ")))
}

async fn nix(args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("nix")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .await
        .wrap_err("running nix")?;
    if !output.status.success() {
        bail!("nix {} failed with {}", args.join(" "), output.status);
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests;
