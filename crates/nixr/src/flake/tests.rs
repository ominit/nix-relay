use serde_json::json;

use super::*;

fn paths(paths: &[&[&str]]) -> Vec<Vec<String>> {
    paths
        .iter()
        .map(|path| path.iter().map(|&part| part.to_owned()).collect())
        .collect()
}

#[test]
fn an_attribute_is_looked_up_like_nix_build_does() {
    assert_eq!(
        attributes("hello", "x86_64-linux").unwrap(),
        paths(&[
            &["packages", "x86_64-linux", "hello"],
            &["legacyPackages", "x86_64-linux", "hello"],
            &["hello"],
        ])
    );
}

#[test]
fn no_attribute_means_the_default_package() {
    assert_eq!(
        attributes("", "aarch64-linux").unwrap(),
        paths(&[
            &[
                "packages",
                "aarch64-linux",
                "packages",
                "aarch64-linux",
                "default"
            ],
            &[
                "legacyPackages",
                "aarch64-linux",
                "packages",
                "aarch64-linux",
                "default"
            ],
            &["packages", "aarch64-linux", "default"],
            &["defaultPackage", "aarch64-linux"],
        ])
    );
}

#[test]
fn a_leading_dot_skips_the_package_prefixes() {
    assert_eq!(
        attributes(
            ".nixosConfigurations.\"tianxuan-1.lan\".config",
            "x86_64-linux"
        )
        .unwrap(),
        paths(&[&["nixosConfigurations", "tianxuan-1.lan", "config"]])
    );
}

#[test]
fn an_unclosed_quote_is_an_error() {
    assert!(attributes("a.\"b", "x86_64-linux").is_err());
}

#[test]
fn sources_include_every_input_once() {
    let archive = json!({
        "path": "/nix/store/root-source",
        "inputs": {
            "nixpkgs": { "path": "/nix/store/nixpkgs-source", "inputs": {} },
            "tool": {
                "path": "/nix/store/tool-source",
                "inputs": { "nixpkgs": { "path": "/nix/store/nixpkgs-source", "inputs": {} } }
            }
        }
    });
    let mut sources = Vec::new();
    collect_sources(&archive, &mut sources);
    assert_eq!(
        sources,
        vec![
            "/nix/store/root-source",
            "/nix/store/nixpkgs-source",
            "/nix/store/tool-source"
        ]
    );
}
