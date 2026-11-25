use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use nix_relay_protocol::{
    BuildId, BuildRequest, BuildResult, ClientEvent, DrvPath, EvalResult, Job, Outcome, Plan,
    PlannedDerivation, RelayMessage, StorePath, WorkerInfo,
};

pub type WorkerId = u64;

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Worker(WorkerId, RelayMessage),
    Client(BuildId, ClientEvent),
}

struct Worker {
    info: WorkerInfo,
    running: HashSet<DrvPath>,
}

impl Worker {
    fn can_build(&self, drv: &PlannedDerivation) -> bool {
        (drv.system == "builtin" || self.info.systems.contains(&drv.system))
            && drv.features.iter().all(|f| self.info.features.contains(f))
    }

    fn has_slot(&self) -> bool {
        self.running.len() < self.info.jobs.max(1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeState {
    Waiting,
    Ready,
    Running(WorkerId),
    Done,
}

struct Node {
    spec: PlannedDerivation,
    state: NodeState,
    waiting_on: HashSet<DrvPath>,
    dependents: HashSet<DrvPath>,
    builds: BTreeSet<BuildId>,
    built_by: Option<WorkerId>,
}

enum Phase {
    Queued,
    Evaluating(WorkerId),
    Building {
        outputs: Vec<String>,
        remaining: HashSet<DrvPath>,
    },
}

struct Build {
    request: BuildRequest,
    phase: Phase,
}

pub struct Scheduler {
    cache_url: String,
    workers: BTreeMap<WorkerId, Worker>,
    builds: BTreeMap<BuildId, Build>,
    protected: BTreeMap<BuildId, Vec<StorePath>>,
    nodes: HashMap<DrvPath, Node>,
    ready: VecDeque<DrvPath>,
}

impl Scheduler {
    pub fn new(cache_url: String) -> Self {
        Self {
            cache_url,
            workers: BTreeMap::new(),
            builds: BTreeMap::new(),
            protected: BTreeMap::new(),
            nodes: HashMap::new(),
            ready: VecDeque::new(),
        }
    }

    pub fn eval_worker(&self, build: BuildId) -> Option<WorkerId> {
        match self.builds.get(&build)?.phase {
            Phase::Evaluating(worker) => Some(worker),
            Phase::Queued | Phase::Building { .. } => None,
        }
    }

    pub fn add_worker(&mut self, id: WorkerId, info: WorkerInfo) -> Vec<Action> {
        let mut actions = Vec::new();
        if info.cache_host {
            for (&build, paths) in &self.protected {
                actions.push(Action::Worker(
                    id,
                    RelayMessage::Protect {
                        build,
                        paths: paths.clone(),
                    },
                ));
            }
        }
        self.workers.insert(
            id,
            Worker {
                info,
                running: HashSet::new(),
            },
        );
        actions.extend(self.dispatch());
        actions
    }

    pub fn remove_worker(&mut self, id: WorkerId) -> Vec<Action> {
        if self.workers.remove(&id).is_none() {
            return Vec::new();
        }
        for build in self.builds.values_mut() {
            if matches!(build.phase, Phase::Evaluating(worker) if worker == id) {
                build.phase = Phase::Queued;
            }
        }
        let mut requeued: Vec<DrvPath> = self
            .nodes
            .iter_mut()
            .filter(|(_, node)| node.state == NodeState::Running(id))
            .map(|(drv, node)| {
                node.state = NodeState::Ready;
                drv.clone()
            })
            .collect();
        requeued.sort();
        self.ready.extend(requeued);
        self.dispatch()
    }

    pub fn submit(&mut self, build: BuildId, request: BuildRequest) -> Vec<Action> {
        self.builds.insert(
            build,
            Build {
                request,
                phase: Phase::Queued,
            },
        );
        self.dispatch()
    }

    pub fn upload_requested(
        &self,
        worker: WorkerId,
        build: BuildId,
        paths: Vec<String>,
    ) -> Vec<Action> {
        if self.eval_worker(build) == Some(worker) {
            vec![Action::Client(build, ClientEvent::Upload { paths })]
        } else {
            Vec::new()
        }
    }

    pub fn upload_done(&self, build: BuildId) -> Vec<Action> {
        self.eval_worker(build)
            .map(|worker| vec![Action::Worker(worker, RelayMessage::UploadDone { build })])
            .unwrap_or_default()
    }

    pub fn eval_log(&self, worker: WorkerId, build: BuildId, line: String) -> Vec<Action> {
        if self.eval_worker(build) == Some(worker) {
            vec![Action::Client(build, ClientEvent::Log { drv: None, line })]
        } else {
            Vec::new()
        }
    }

    pub fn build_log(&self, drv: &str, line: &str) -> Vec<Action> {
        self.nodes
            .get(drv)
            .map(|node| {
                node.builds
                    .iter()
                    .map(|&build| {
                        Action::Client(
                            build,
                            ClientEvent::Log {
                                drv: Some(drv.to_owned()),
                                line: line.to_owned(),
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn client_gone(&mut self, build: BuildId) -> Vec<Action> {
        let mut actions = Vec::new();
        let mut holders = self.cache_hosts();
        if let Some(removed) = self.builds.remove(&build) {
            if let Phase::Evaluating(worker) = removed.phase {
                actions.push(Action::Worker(worker, RelayMessage::UploadDone { build }));
                holders.insert(worker);
            }
            self.detach(build);
        }
        self.protected.remove(&build);
        for worker in holders {
            actions.push(Action::Worker(worker, RelayMessage::Release { build }));
        }
        actions.extend(self.dispatch());
        actions
    }

    fn cache_hosts(&self) -> BTreeSet<WorkerId> {
        self.workers
            .iter()
            .filter(|(_, w)| w.info.cache_host)
            .map(|(&id, _)| id)
            .collect()
    }

    fn protect(&mut self, build: BuildId, plan: &Plan, actions: &mut Vec<Action>) {
        let paths: BTreeSet<&StorePath> = plan
            .outputs
            .iter()
            .chain(&plan.available)
            .chain(
                plan.derivations
                    .iter()
                    .flat_map(|d| std::iter::once(&d.drv).chain(&d.outputs)),
            )
            .collect();
        let paths: Vec<StorePath> = paths.into_iter().cloned().collect();
        for worker in self.cache_hosts() {
            actions.push(Action::Worker(
                worker,
                RelayMessage::Protect {
                    build,
                    paths: paths.clone(),
                },
            ));
        }
        self.protected.insert(build, paths);
    }

    pub fn evaluated(
        &mut self,
        worker: WorkerId,
        build: BuildId,
        result: EvalResult,
    ) -> Vec<Action> {
        if self.eval_worker(build) != Some(worker) {
            return Vec::new();
        }
        let mut actions = Vec::new();
        match result {
            EvalResult::Failed { error } => {
                self.finish(build, Outcome::Failed { error }, &mut actions)
            }
            EvalResult::Planned { plan } => {
                self.protect(build, &plan, &mut actions);
                self.merge(build, plan, &mut actions);
            }
        }
        actions.extend(self.dispatch());
        actions
    }

    pub fn built(&mut self, worker: WorkerId, drv: &str, result: BuildResult) -> Vec<Action> {
        if let Some(w) = self.workers.get_mut(&worker) {
            w.running.remove(drv);
        }
        let mut actions = Vec::new();
        let Some(node) = self.nodes.get_mut(drv) else {
            return self.dispatch();
        };
        if node.state == NodeState::Done {
            return self.dispatch();
        }
        match result {
            BuildResult::Succeeded => {
                node.state = NodeState::Done;
                node.built_by = Some(worker);
                let builds: Vec<BuildId> = node.builds.iter().copied().collect();
                let mut dependents: Vec<DrvPath> = node.dependents.iter().cloned().collect();
                dependents.sort();
                for build in builds {
                    actions.push(Action::Client(
                        build,
                        ClientEvent::Built {
                            drv: drv.to_owned(),
                        },
                    ));
                    let finished = match self.builds.get_mut(&build) {
                        Some(Build {
                            phase: Phase::Building { remaining, outputs },
                            ..
                        }) => {
                            remaining.remove(drv);
                            remaining.is_empty().then(|| outputs.clone())
                        }
                        Some(_) | None => None,
                    };
                    if let Some(outputs) = finished {
                        let outcome = Outcome::Succeeded {
                            outputs,
                            fetch_from: Some(self.cache_url.clone()),
                        };
                        self.finish(build, outcome, &mut actions);
                    }
                }
                for dependent in dependents {
                    self.satisfy(&dependent, drv);
                }
            }
            BuildResult::Failed { error } => {
                let name = self
                    .workers
                    .get(&worker)
                    .map_or_else(|| worker.to_string(), |w| w.info.name.clone());
                let error = format!("{drv} failed on {name}:\n{error}");
                let builds: Vec<BuildId> = node.builds.iter().copied().collect();
                for build in builds {
                    self.finish(
                        build,
                        Outcome::Failed {
                            error: error.clone(),
                        },
                        &mut actions,
                    );
                }
            }
        }
        actions.extend(self.dispatch());
        actions
    }

    fn merge(&mut self, build: BuildId, plan: Plan, actions: &mut Vec<Action>) {
        let count = plan.derivations.len();
        actions.push(Action::Client(
            build,
            ClientEvent::Planned { derivations: count },
        ));
        let mut added = Vec::new();
        for spec in plan.derivations {
            let drv = spec.drv.clone();
            match self.nodes.get_mut(&drv) {
                Some(node) => {
                    node.builds.insert(build);
                }
                None => {
                    self.nodes.insert(
                        drv.clone(),
                        Node {
                            spec,
                            state: NodeState::Waiting,
                            waiting_on: HashSet::new(),
                            dependents: HashSet::new(),
                            builds: BTreeSet::from([build]),
                            built_by: None,
                        },
                    );
                    added.push(drv);
                }
            }
        }
        added.sort();
        for drv in &added {
            let inputs = self
                .nodes
                .get(drv)
                .map(|node| node.spec.inputs.clone())
                .unwrap_or_default();
            let mut waiting_on = HashSet::new();
            for input in inputs {
                if let Some(dep) = self.nodes.get_mut(&input)
                    && dep.state != NodeState::Done
                {
                    dep.dependents.insert(drv.clone());
                    waiting_on.insert(input);
                }
            }
            if let Some(node) = self.nodes.get_mut(drv) {
                if waiting_on.is_empty() {
                    node.state = NodeState::Ready;
                    self.ready.push_back(drv.clone());
                }
                node.waiting_on = waiting_on;
            }
        }
        let remaining: HashSet<DrvPath> = self
            .nodes
            .iter()
            .filter(|(_, node)| node.builds.contains(&build) && node.state != NodeState::Done)
            .map(|(drv, _)| drv.clone())
            .collect();
        if remaining.is_empty() {
            let fetch_from = (count > 0).then(|| self.cache_url.clone());
            self.finish(
                build,
                Outcome::Succeeded {
                    outputs: plan.outputs,
                    fetch_from,
                },
                actions,
            );
        } else if let Some(b) = self.builds.get_mut(&build) {
            b.phase = Phase::Building {
                outputs: plan.outputs,
                remaining,
            };
        }
    }

    fn satisfy(&mut self, dependent: &str, dependency: &str) {
        let Some(node) = self.nodes.get_mut(dependent) else {
            return;
        };
        node.waiting_on.remove(dependency);
        if node.waiting_on.is_empty() && node.state == NodeState::Waiting {
            node.state = NodeState::Ready;
            self.ready.push_back(dependent.to_owned());
        }
    }

    fn finish(&mut self, build: BuildId, outcome: Outcome, actions: &mut Vec<Action>) {
        if self.builds.remove(&build).is_some() {
            self.detach(build);
            actions.push(Action::Client(build, ClientEvent::Finished { outcome }));
        }
    }

    fn detach(&mut self, build: BuildId) {
        let mut orphaned = Vec::new();
        for (drv, node) in &mut self.nodes {
            if node.builds.remove(&build) && node.builds.is_empty() {
                orphaned.push(drv.clone());
            }
        }
        orphaned.sort();
        for drv in orphaned {
            let Some(node) = self.nodes.remove(&drv) else {
                continue;
            };
            let mut dependents: Vec<DrvPath> = node.dependents.into_iter().collect();
            dependents.sort();
            for dependent in dependents {
                self.satisfy(&dependent, &drv);
            }
        }
    }

    fn dispatch(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        self.dispatch_evals(&mut actions);
        self.dispatch_jobs(&mut actions);
        actions
    }

    fn dispatch_evals(&mut self, actions: &mut Vec<Action>) {
        let Some(worker) = self
            .workers
            .iter()
            .find(|(_, w)| w.info.cache_host)
            .or_else(|| self.workers.iter().next())
            .map(|(&id, _)| id)
        else {
            return;
        };
        let name = self
            .workers
            .get(&worker)
            .map(|w| w.info.name.clone())
            .unwrap_or_default();
        for (&id, build) in &mut self.builds {
            if matches!(build.phase, Phase::Queued) {
                build.phase = Phase::Evaluating(worker);
                actions.push(Action::Worker(
                    worker,
                    RelayMessage::Eval {
                        build: id,
                        request: build.request.clone(),
                    },
                ));
                actions.push(Action::Client(
                    id,
                    ClientEvent::Evaluating {
                        worker: name.clone(),
                    },
                ));
            }
        }
    }

    fn dispatch_jobs(&mut self, actions: &mut Vec<Action>) {
        let mut deferred = VecDeque::new();
        while let Some(drv) = self.ready.pop_front() {
            let Some(node) = self.nodes.get(&drv) else {
                continue;
            };
            if node.state != NodeState::Ready {
                continue;
            }
            let Some(worker) = self.pick_worker(node) else {
                deferred.push_back(drv);
                continue;
            };
            let job = Job {
                drv: drv.clone(),
                outputs: node.spec.outputs.clone(),
                fetch: node.spec.fetch.clone(),
            };
            let builds: Vec<BuildId> = node.builds.iter().copied().collect();
            let name = match self.workers.get_mut(&worker) {
                Some(w) => {
                    w.running.insert(drv.clone());
                    w.info.name.clone()
                }
                None => continue,
            };
            if let Some(node) = self.nodes.get_mut(&drv) {
                node.state = NodeState::Running(worker);
            }
            actions.push(Action::Worker(worker, RelayMessage::Build { job }));
            for build in builds {
                actions.push(Action::Client(
                    build,
                    ClientEvent::Started {
                        drv: drv.clone(),
                        worker: name.clone(),
                    },
                ));
            }
        }
        self.ready = deferred;
    }

    fn pick_worker(&self, node: &Node) -> Option<WorkerId> {
        let local_only = node.spec.prefer_local
            && self
                .workers
                .values()
                .any(|w| w.info.cache_host && w.can_build(&node.spec));
        self.workers
            .iter()
            .filter(|(_, w)| w.can_build(&node.spec) && w.has_slot())
            .filter(|(_, w)| !local_only || w.info.cache_host)
            .max_by_key(|&(&id, w)| {
                let locality = node
                    .spec
                    .inputs
                    .iter()
                    .filter(|input| {
                        self.nodes
                            .get(*input)
                            .and_then(|n| n.built_by)
                            .is_some_and(|by| by == id)
                    })
                    .count();
                (
                    locality,
                    std::cmp::Reverse(w.running.len()),
                    std::cmp::Reverse(id),
                )
            })
            .map(|(&id, _)| id)
    }
}

#[cfg(test)]
mod tests;
