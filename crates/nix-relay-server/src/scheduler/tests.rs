use super::*;

const CACHE: &str = "http://cache";

fn info(name: &str, cache_host: bool, jobs: usize) -> WorkerInfo {
    WorkerInfo {
        name: name.to_owned(),
        systems: vec!["x86_64-linux".to_owned()],
        features: vec!["big-parallel".to_owned()],
        jobs,
        cache_host,
    }
}

fn path(name: &str) -> String {
    format!("/nix/store/{name}.drv")
}

fn drv(name: &str, inputs: &[&str]) -> PlannedDerivation {
    PlannedDerivation {
        drv: path(name),
        system: "x86_64-linux".to_owned(),
        features: Vec::new(),
        prefer_local: false,
        outputs: vec![format!("/nix/store/{name}")],
        inputs: inputs.iter().map(|i| path(i)).collect(),
        fetch: inputs.iter().map(|i| format!("/nix/store/{i}")).collect(),
    }
}

fn request() -> BuildRequest {
    BuildRequest::Flake {
        url: "path:/nix/store/source".to_owned(),
        attribute: "default".to_owned(),
        sources: Vec::new(),
    }
}

fn planned(derivations: Vec<PlannedDerivation>) -> EvalResult {
    EvalResult::Planned {
        plan: Plan {
            outputs: vec!["/nix/store/root".to_owned()],
            derivations,
        },
    }
}

fn jobs(actions: &[Action]) -> Vec<(WorkerId, String)> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Worker(worker, RelayMessage::Build { job }) => Some((*worker, job.drv.clone())),
            Action::Worker(..) | Action::Client(..) => None,
        })
        .collect()
}

fn finished(actions: &[Action], build: BuildId) -> Option<Outcome> {
    actions.iter().find_map(|action| match action {
        Action::Client(b, ClientEvent::Finished { outcome }) if *b == build => {
            Some(outcome.clone())
        }
        Action::Worker(..) | Action::Client(..) => None,
    })
}

fn evaluating(s: &mut Scheduler, build: BuildId) -> WorkerId {
    s.submit(build, request());
    s.eval_worker(build).unwrap()
}

#[test]
fn evaluation_waits_for_a_worker_and_prefers_the_cache_host() {
    let mut s = Scheduler::new(CACHE.to_owned());
    let actions = s.submit(1, request());
    assert!(actions.is_empty());
    s.add_worker(10, info("w1", false, 1));
    assert_eq!(s.eval_worker(1), Some(10));

    s.submit(2, request());
    s.remove_worker(10);
    s.add_worker(11, info("w2", false, 1));
    s.add_worker(12, info("cache", true, 1));
    s.submit(3, request());
    assert_eq!(s.eval_worker(1), Some(11));
    assert_eq!(s.eval_worker(3), Some(12));
}

#[test]
fn a_chain_builds_in_dependency_order_and_finishes() {
    let mut s = Scheduler::new(CACHE.to_owned());
    s.add_worker(1, info("w", true, 4));
    let w = evaluating(&mut s, 7);

    let actions = s.evaluated(w, 7, planned(vec![drv("b", &["a"]), drv("a", &[])]));
    assert_eq!(jobs(&actions), vec![(1, path("a"))]);

    let actions = s.built(1, &path("a"), BuildResult::Succeeded);
    assert_eq!(jobs(&actions), vec![(1, path("b"))]);
    assert_eq!(finished(&actions, 7), None);

    let actions = s.built(1, &path("b"), BuildResult::Succeeded);
    assert_eq!(
        finished(&actions, 7),
        Some(Outcome::Succeeded {
            outputs: vec!["/nix/store/root".to_owned()],
            fetch_from: Some(CACHE.to_owned()),
        })
    );
}

#[test]
fn an_empty_plan_finishes_without_fetching_from_the_cache() {
    let mut s = Scheduler::new(CACHE.to_owned());
    s.add_worker(1, info("w", true, 1));
    let w = evaluating(&mut s, 1);
    let actions = s.evaluated(w, 1, planned(Vec::new()));
    assert_eq!(
        finished(&actions, 1),
        Some(Outcome::Succeeded {
            outputs: vec!["/nix/store/root".to_owned()],
            fetch_from: None,
        })
    );
}

#[test]
fn independent_derivations_spread_over_free_slots() {
    let mut s = Scheduler::new(CACHE.to_owned());
    s.add_worker(1, info("w1", true, 1));
    s.add_worker(2, info("w2", false, 1));
    let w = evaluating(&mut s, 1);
    let actions = s.evaluated(
        w,
        1,
        planned(vec![
            drv("a", &[]),
            drv("b", &[]),
            drv("c", &[]),
            drv("d", &["a", "b", "c"]),
        ]),
    );
    let dispatched = jobs(&actions);
    assert_eq!(dispatched.len(), 2);
    assert_ne!(dispatched[0].0, dispatched[1].0);

    let actions = s.built(dispatched[0].0, &dispatched[0].1, BuildResult::Succeeded);
    assert_eq!(jobs(&actions), vec![(dispatched[0].0, path("c"))]);
}

#[test]
fn a_derivation_goes_to_the_worker_that_built_its_inputs() {
    let mut s = Scheduler::new(CACHE.to_owned());
    s.add_worker(1, info("w1", true, 1));
    s.add_worker(2, info("w2", false, 1));
    let w = evaluating(&mut s, 1);
    let actions = s.evaluated(
        w,
        1,
        planned(vec![drv("a", &[]), drv("x", &[]), drv("b", &["x"])]),
    );
    assert_eq!(jobs(&actions), vec![(1, path("a")), (2, path("x"))]);

    assert!(jobs(&s.built(1, &path("a"), BuildResult::Succeeded)).is_empty());
    let actions = s.built(2, &path("x"), BuildResult::Succeeded);
    assert_eq!(jobs(&actions), vec![(2, path("b"))]);
}

#[test]
fn prefer_local_derivations_wait_for_the_cache_host() {
    let mut s = Scheduler::new(CACHE.to_owned());
    s.add_worker(1, info("cache", true, 1));
    s.add_worker(2, info("w2", false, 4));
    let w = evaluating(&mut s, 1);
    let mut small = drv("small", &[]);
    small.prefer_local = true;
    let mut other = drv("other", &[]);
    other.prefer_local = true;
    let actions = s.evaluated(w, 1, planned(vec![small, other]));
    assert_eq!(jobs(&actions), vec![(1, path("other"))]);

    let actions = s.built(1, &path("other"), BuildResult::Succeeded);
    assert_eq!(jobs(&actions), vec![(1, path("small"))]);
}

#[test]
fn derivations_only_go_to_workers_with_the_right_system_and_features() {
    let mut s = Scheduler::new(CACHE.to_owned());
    s.add_worker(1, info("w", true, 4));
    let w = evaluating(&mut s, 1);
    let mut darwin = drv("darwin", &[]);
    darwin.system = "aarch64-darwin".to_owned();
    let mut kvm = drv("kvm", &[]);
    kvm.features = vec!["kvm".to_owned()];
    let mut big = drv("big", &[]);
    big.features = vec!["big-parallel".to_owned()];
    let mut fetch = drv("fetch", &[]);
    fetch.system = "builtin".to_owned();
    let actions = s.evaluated(w, 1, planned(vec![darwin, kvm, big, fetch]));
    let mut dispatched: Vec<String> = jobs(&actions).into_iter().map(|(_, d)| d).collect();
    dispatched.sort();
    assert_eq!(dispatched, vec![path("big"), path("fetch")]);
}

#[test]
fn a_disconnected_worker_hands_its_jobs_to_another() {
    let mut s = Scheduler::new(CACHE.to_owned());
    s.add_worker(1, info("w1", true, 1));
    let w = evaluating(&mut s, 1);
    s.evaluated(w, 1, planned(vec![drv("a", &[])]));

    s.add_worker(2, info("w2", false, 1));
    let actions = s.remove_worker(1);
    assert_eq!(jobs(&actions), vec![(2, path("a"))]);
}

#[test]
fn a_worker_lost_during_evaluation_requeues_the_evaluation() {
    let mut s = Scheduler::new(CACHE.to_owned());
    s.add_worker(1, info("w1", true, 1));
    s.add_worker(2, info("w2", false, 1));
    evaluating(&mut s, 1);
    s.remove_worker(1);
    assert_eq!(s.eval_worker(1), Some(2));
    assert!(s.evaluated(1, 1, planned(Vec::new())).is_empty());
}

#[test]
fn a_failure_fails_every_build_that_needs_it() {
    let mut s = Scheduler::new(CACHE.to_owned());
    s.add_worker(1, info("w", true, 4));
    let w = evaluating(&mut s, 1);
    s.evaluated(w, 1, planned(vec![drv("a", &[]), drv("b", &["a"])]));
    let w = evaluating(&mut s, 2);
    s.evaluated(w, 2, planned(vec![drv("a", &[])]));

    let actions = s.built(
        1,
        &path("a"),
        BuildResult::Failed {
            error: "exit 1".to_owned(),
        },
    );
    assert!(jobs(&actions).is_empty());
    assert!(matches!(
        finished(&actions, 1),
        Some(Outcome::Failed { .. })
    ));
    assert!(matches!(
        finished(&actions, 2),
        Some(Outcome::Failed { .. })
    ));
}

#[test]
fn builds_share_derivations_instead_of_building_them_twice() {
    let mut s = Scheduler::new(CACHE.to_owned());
    s.add_worker(1, info("w", true, 4));
    let w = evaluating(&mut s, 1);
    let first = s.evaluated(w, 1, planned(vec![drv("a", &[])]));
    let w = evaluating(&mut s, 2);
    let second = s.evaluated(w, 2, planned(vec![drv("a", &[])]));
    assert_eq!(jobs(&first).len(), 1);
    assert!(jobs(&second).is_empty());

    let actions = s.built(1, &path("a"), BuildResult::Succeeded);
    assert!(finished(&actions, 1).is_some());
    assert!(finished(&actions, 2).is_some());
}

#[test]
fn a_cancelled_build_releases_dependents_that_another_build_planned_without_it() {
    let mut s = Scheduler::new(CACHE.to_owned());
    s.add_worker(1, info("w", true, 2));
    let w = evaluating(&mut s, 1);
    s.evaluated(w, 1, planned(vec![drv("a", &[]), drv("b", &["a"])]));
    let w = evaluating(&mut s, 2);
    s.evaluated(w, 2, planned(vec![drv("b", &[])]));
    let actions = s.client_gone(1);
    assert_eq!(jobs(&actions), vec![(1, path("b"))]);
}

#[test]
fn results_from_unknown_derivations_free_the_slot() {
    let mut s = Scheduler::new(CACHE.to_owned());
    s.add_worker(1, info("w", true, 1));
    let w = evaluating(&mut s, 1);
    s.evaluated(w, 1, planned(vec![drv("a", &[]), drv("b", &[])]));
    s.client_gone(1);
    let w = evaluating(&mut s, 2);
    let actions = s.evaluated(w, 2, planned(vec![drv("c", &[])]));
    assert!(jobs(&actions).is_empty());
    let actions = s.built(1, &path("a"), BuildResult::Succeeded);
    assert_eq!(jobs(&actions), vec![(1, path("c"))]);
}

#[test]
fn a_client_leaving_during_evaluation_ends_its_upload() {
    let mut s = Scheduler::new(CACHE.to_owned());
    s.add_worker(1, info("w", true, 1));
    let w = evaluating(&mut s, 5);
    let actions = s.client_gone(5);
    assert_eq!(
        actions,
        vec![Action::Worker(w, RelayMessage::UploadDone { build: 5 })]
    );
}
