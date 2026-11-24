use std::{collections::HashSet, path::Path};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

use super::*;

fn out(name: &str) -> String {
    format!("/nix/store/{name:0>32}-{name}")
}

fn write_drv(dir: &Path, name: &str, inputs: &[(&str, &[&str])], env: &str) -> String {
    let path = dir.join(format!("{name}.drv"));
    let inputs: Vec<String> = inputs
        .iter()
        .map(|(input, outputs)| {
            let outputs: Vec<String> = outputs.iter().map(|o| format!("\"{o}\"")).collect();
            format!(
                "(\"{}\",[{}])",
                dir.join(format!("{input}.drv")).display(),
                outputs.join(",")
            )
        })
        .collect();
    let contents = format!(
        "Derive([(\"dev\",\"{}\",\"\",\"\"),(\"out\",\"{}\",\"\",\"\")],[{}],[],\"x86_64-linux\",\"/bin/sh\",[],[{env}])",
        out(&format!("{name}-dev")),
        out(name),
        inputs.join(",")
    );
    std::fs::write(&path, contents).unwrap();
    path.display().to_string()
}

async fn cache(available: &[String]) -> String {
    let hashes: Arc<HashSet<String>> = Arc::new(
        available
            .iter()
            .map(|path| {
                path.trim_start_matches("/nix/store/")
                    .get(..32)
                    .unwrap()
                    .to_owned()
            })
            .collect(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let hashes = hashes.clone();
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut lines = BufReader::new(read).lines();
                while let Ok(Some(request)) = lines.next_line().await {
                    while let Ok(Some(header)) = lines.next_line().await {
                        if header.is_empty() {
                            break;
                        }
                    }
                    let hash = request
                        .split_whitespace()
                        .nth(1)
                        .and_then(|p| p.strip_prefix('/'))
                        .and_then(|p| p.strip_suffix(".narinfo"))
                        .unwrap_or_default();
                    let status = if hashes.contains(hash) {
                        "200 OK"
                    } else {
                        "404 Not Found"
                    };
                    let response = format!("HTTP/1.1 {status}\r\ncontent-length: 0\r\n\r\n");
                    if write.write_all(response.as_bytes()).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    format!("http://{addr}")
}

fn names(plan: &Plan) -> Vec<String> {
    plan.derivations
        .iter()
        .map(|d| {
            Path::new(&d.drv)
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

#[tokio::test]
async fn plans_only_what_no_cache_has_and_stops_at_cached_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    write_drv(d, "deep", &[], "");
    write_drv(d, "cached", &[("deep", &["out"])], "");
    write_drv(d, "lib", &[], r#"("preferLocalBuild","1")"#);
    let root = write_drv(d, "root", &[("cached", &["out"]), ("lib", &["dev"])], "");

    let url = cache(&[out("cached")]).await;
    let planner = Planner::new(vec![url]).unwrap();
    let plan = planner
        .plan(vec![(root, BTreeSet::from(["out".to_owned()]))])
        .await
        .unwrap();

    assert_eq!(plan.outputs, vec![out("root")]);
    assert_eq!(names(&plan), vec!["lib", "root"]);
    let root = plan
        .derivations
        .iter()
        .find(|p| p.drv.ends_with("root.drv"))
        .unwrap();
    assert_eq!(root.inputs, vec![d.join("lib.drv").display().to_string()]);
    assert_eq!(root.fetch, vec![out("lib-dev")]);
    let lib = plan
        .derivations
        .iter()
        .find(|p| p.drv.ends_with("lib.drv"))
        .unwrap();
    assert!(lib.prefer_local);
    assert_eq!(lib.outputs, vec![out("lib-dev"), out("lib")]);
}

#[tokio::test]
async fn a_cached_root_needs_no_builds() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_drv(dir.path(), "root", &[], "");
    let url = cache(&[out("root")]).await;
    let plan = Planner::new(vec![url])
        .unwrap()
        .plan(vec![(root, BTreeSet::from(["out".to_owned()]))])
        .await
        .unwrap();
    assert!(plan.derivations.is_empty());
}

#[tokio::test]
async fn a_missing_output_of_a_partly_cached_derivation_is_built() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    write_drv(d, "lib", &[], "");
    let root = write_drv(d, "root", &[("lib", &["out", "dev"])], "");
    let url = cache(&[out("lib")]).await;
    let plan = Planner::new(vec![url])
        .unwrap()
        .plan(vec![(root, BTreeSet::from(["out".to_owned()]))])
        .await
        .unwrap();
    assert_eq!(names(&plan), vec!["lib", "root"]);
}

#[tokio::test]
async fn later_caches_are_checked_when_earlier_ones_miss() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_drv(dir.path(), "root", &[], "");
    let empty = cache(&[]).await;
    let full = cache(&[out("root")]).await;
    let plan = Planner::new(vec![empty, full])
        .unwrap()
        .plan(vec![(root, BTreeSet::from(["out".to_owned()]))])
        .await
        .unwrap();
    assert!(plan.derivations.is_empty());
}

#[test]
fn cache_urls_keep_http_caches_once_without_query_or_trailing_slash() {
    let urls = cache_urls(
        "http://mayohiga:5000/",
        "http://mayohiga:5000 https://cache.nixos.org/?priority=40 file:///tmp/cache http://localhost:8080",
    );
    assert_eq!(
        urls,
        vec![
            "http://mayohiga:5000",
            "https://cache.nixos.org",
            "http://localhost:8080"
        ]
    );
}

#[test]
fn roots_come_from_dry_run_json() {
    let json = r#"[{"drvPath":"/nix/store/x-b.drv","outputs":{"out":"/nix/store/y-b","dev":"/nix/store/z-b-dev"}}]"#;
    assert_eq!(
        roots(json).unwrap(),
        vec![(
            "/nix/store/x-b.drv".to_owned(),
            BTreeSet::from(["dev".to_owned(), "out".to_owned()])
        )]
    );
}
