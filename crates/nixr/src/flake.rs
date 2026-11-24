use std::process::Stdio;

use color_eyre::eyre::{Result, WrapErr, bail, eyre};
use nix_relay_protocol::BuildRequest;
use serde_json::Value;
use tokio::process::Command;

const LOCKED_ATTRS: [&str; 5] = ["rev", "revCount", "lastModified", "narHash", "dir"];

pub async fn request(installable: &str) -> Result<BuildRequest> {
    let (flake, attribute) = installable.split_once('#').unwrap_or((installable, ""));

    let archive = nix_json(&["flake", "archive", "--json", flake]).await?;
    let metadata = nix_json(&["flake", "metadata", "--json", flake]).await?;

    let path = archive
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| eyre!("nix flake archive did not report a source path"))?;
    let mut sources = Vec::new();
    collect_sources(&archive, &mut sources);

    let locked = metadata
        .get("locked")
        .ok_or_else(|| eyre!("nix flake metadata did not report a locked reference"))?;
    Ok(BuildRequest::Flake {
        url: source_url(path, locked),
        attribute: attribute.to_owned(),
        sources,
    })
}

pub fn source_url(path: &str, locked: &Value) -> String {
    let query: Vec<String> = LOCKED_ATTRS
        .iter()
        .filter_map(|&key| {
            let value = match locked.get(key)? {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_) => return None,
            };
            Some(format!("{key}={}", encode(&value)))
        })
        .collect();
    if query.is_empty() {
        format!("path:{path}")
    } else {
        format!("path:{path}?{}", query.join("&"))
    }
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

fn encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

async fn nix_json(args: &[&str]) -> Result<Value> {
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
    serde_json::from_slice(&output.stdout)
        .wrap_err_with(|| format!("parsing nix {} output", args.join(" ")))
}

#[cfg(test)]
mod tests;
