use super::*;

#[test]
fn roots_are_symlinks_per_key_and_go_away_on_release() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("roots");
    std::fs::create_dir_all(dir.join("builds/9")).unwrap();
    let roots = Roots::new(dir.clone());
    assert!(
        !dir.join("builds/9").exists(),
        "a new session starts with no roots"
    );

    let a = "/nix/store/aaaa-a".to_owned();
    let b = "/nix/store/bbbb-b.drv".to_owned();
    let key = Roots::build_key(1);
    roots.protect(&key, [&a, &b]);
    roots.protect(&key, [&a]);
    assert_eq!(
        std::fs::read_link(dir.join("builds/1/aaaa-a")).unwrap(),
        Path::new(&a)
    );
    assert_eq!(
        std::fs::read_link(dir.join("builds/1/bbbb-b.drv")).unwrap(),
        Path::new(&b)
    );

    let job = Roots::job_key(&b);
    assert_eq!(job, "jobs/bbbb-b.drv");
    roots.protect(&job, [&a]);
    roots.release(&key);
    roots.release(&key);
    assert!(!dir.join("builds/1").exists());
    assert!(dir.join("jobs/bbbb-b.drv/aaaa-a").is_symlink());
}
