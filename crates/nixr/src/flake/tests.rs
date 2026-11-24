use serde_json::json;

use super::*;

#[test]
fn source_urls_carry_the_locked_revision_and_hash() {
    let locked = json!({
        "__final": true,
        "lastModified": 1795500150,
        "narHash": "sha256-5WIl1Nf5XaMM5oGGV3fMQ2knoACPeukbH2YNOKIdTUg=",
        "ref": "refs/heads/master",
        "rev": "435657b1c366406fd48289730964f8ef96a71d77",
        "revCount": 1,
        "type": "git",
        "url": "file:///home/me/project"
    });
    assert_eq!(
        source_url(
            "/nix/store/bfsvdil3anr5q6di2pl64cqxd7vwm5p4-source",
            &locked
        ),
        "path:/nix/store/bfsvdil3anr5q6di2pl64cqxd7vwm5p4-source?rev=435657b1c366406fd48289730964f8ef96a71d77&revCount=1&lastModified=1795500150&narHash=sha256-5WIl1Nf5XaMM5oGGV3fMQ2knoACPeukbH2YNOKIdTUg%3D"
    );
}

#[test]
fn dirty_trees_drop_the_dirty_revision() {
    let locked = json!({
        "dirtyRev": "435657b1c366406fd48289730964f8ef96a71d77-dirty",
        "lastModified": 1795500150,
        "narHash": "sha256-eybyAr/fhI0r4sV87Tvx0TECyo/XxvcuV/16lCachrA=",
        "type": "git"
    });
    assert_eq!(
        source_url(
            "/nix/store/a39qgl79mz662a21qg0j3jm2b33ndbm0-source",
            &locked
        ),
        "path:/nix/store/a39qgl79mz662a21qg0j3jm2b33ndbm0-source?lastModified=1795500150&narHash=sha256-eybyAr%2FfhI0r4sV87Tvx0TECyo%2FXxvcuV%2F16lCachrA%3D"
    );
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
