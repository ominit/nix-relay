use super::*;

const SIMPLE: &str = r#"Derive([("out","/nix/store/kdfjzy4rnqm7mmw0w8a46rsamflw5lfx-b","","")],[("/nix/store/11rln2p1mv0mrnp5hw11rncw58wzi3l9-a.drv",["out"])],[],"x86_64-linux","/bin/sh",["-c","echo b > $out"],[("builder","/bin/sh"),("name","b"),("out","/nix/store/kdfjzy4rnqm7mmw0w8a46rsamflw5lfx-b"),("system","x86_64-linux")])"#;

#[test]
fn parses_outputs_inputs_and_system() {
    let drv = parse(SIMPLE.as_bytes()).unwrap();
    assert_eq!(
        drv.outputs,
        BTreeMap::from([(
            "out".to_owned(),
            "/nix/store/kdfjzy4rnqm7mmw0w8a46rsamflw5lfx-b".to_owned()
        )])
    );
    assert_eq!(
        drv.input_drvs,
        BTreeMap::from([(
            "/nix/store/11rln2p1mv0mrnp5hw11rncw58wzi3l9-a.drv".to_owned(),
            BTreeSet::from(["out".to_owned()])
        )])
    );
    assert_eq!(drv.system, "x86_64-linux");
    assert_eq!(drv.env.get("name").map(String::as_str), Some("b"));
}

#[test]
fn unescapes_strings() {
    let input = r#"Derive([],[],[],"x86_64-linux","/bin/sh",[],[("script","line \"one\"\nline\ttwo \\ end")])"#;
    let drv = parse(input.as_bytes()).unwrap();
    assert_eq!(
        drv.env.get("script").map(String::as_str),
        Some("line \"one\"\nline\ttwo \\ end")
    );
}

#[test]
fn reads_features_and_prefer_local_from_env() {
    let input = r#"Derive([],[],[],"x86_64-linux","/bin/sh",[],[("preferLocalBuild","1"),("requiredSystemFeatures","kvm big-parallel")])"#;
    let drv = parse(input.as_bytes()).unwrap();
    assert!(drv.prefer_local_build());
    assert_eq!(drv.required_features(), vec!["kvm", "big-parallel"]);
}

#[test]
fn reads_features_and_prefer_local_from_structured_attrs() {
    let input = r#"Derive([],[],[],"x86_64-linux","/bin/sh",[],[("__json","{\"preferLocalBuild\":true,\"requiredSystemFeatures\":[\"kvm\"]}")])"#;
    let drv = parse(input.as_bytes()).unwrap();
    assert!(drv.prefer_local_build());
    assert_eq!(drv.required_features(), vec!["kvm"]);
}

#[test]
fn rejects_dynamic_derivations_and_truncated_input() {
    assert!(parse(br#"DrvWithVersion("xp-dyn-drv",[],[])"#).is_err());
    assert!(parse(&SIMPLE.as_bytes()[..SIMPLE.len() - 1]).is_err());
}

#[tokio::test]
#[ignore = "needs nix and network access to evaluate nixpkgs"]
async fn parses_a_real_derivation_from_the_store() {
    let output = tokio::process::Command::new("nix")
        .args(["path-info", "--derivation", "nixpkgs#hello"])
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    let path = String::from_utf8(output.stdout).unwrap();
    let drv = Derivation::read(path.trim()).await.unwrap();
    assert!(drv.outputs.contains_key("out"));
    assert!(!drv.input_drvs.is_empty());
}
