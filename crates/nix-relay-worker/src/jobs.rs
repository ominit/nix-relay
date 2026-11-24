use std::process::Stdio;

use color_eyre::eyre::{Result, WrapErr, bail};
use nix_relay_protocol::{
    BuildId, BuildRequest, BuildResult, EvalResult, Job, Plan, WorkerMessage,
};
use tokio::{io::AsyncWriteExt, sync::mpsc};

use crate::{
    nix::{self, Outbox},
    plan::{self, Planner},
    roots::Roots,
};

pub struct Context {
    pub out: Outbox,
    pub cache_url: String,
    pub cache_store: String,
    pub cache_host: bool,
    pub planner: Planner,
    pub roots: Roots,
}

impl Context {
    pub async fn eval(
        &self,
        build: BuildId,
        request: BuildRequest,
        upload: mpsc::Receiver<Vec<u8>>,
    ) -> EvalResult {
        let result = match request {
            BuildRequest::Flake {
                url,
                attribute,
                sources,
            } => self.flake(build, &url, &attribute, &sources, upload).await,
        };
        match result {
            Ok(plan) => EvalResult::Planned { plan },
            Err(error) => EvalResult::Failed {
                error: format!("{error:#}"),
            },
        }
    }

    async fn flake(
        &self,
        build: BuildId,
        url: &str,
        attribute: &str,
        sources: &[String],
        upload: mpsc::Receiver<Vec<u8>>,
    ) -> Result<Plan> {
        let key = Roots::build_key(build);
        self.roots.protect(&key, sources);
        let missing = invalid_paths(sources).await?;
        if !missing.is_empty() {
            self.out
                .send(WorkerMessage::Upload {
                    build,
                    paths: missing,
                })
                .await?;
            import(upload).await?;
        }

        let installable = if attribute.is_empty() {
            url.to_owned()
        } else {
            format!("{url}#{attribute}")
        };
        let mut command = nix::command("nix");
        command.args([
            "build",
            "--dry-run",
            "--json",
            "--quiet",
            "--option",
            "substitute",
            "false",
            &installable,
        ]);
        let stdout = nix::run(command, &self.out, |line| {
            (!is_dry_run_summary(&line)).then_some(WorkerMessage::EvalLog { build, line })
        })
        .await?;
        let roots = plan::roots(&String::from_utf8_lossy(&stdout))?;
        self.roots.protect(&key, roots.iter().map(|(drv, _)| drv));
        let plan = self.planner.plan(roots).await?;

        if !self.cache_host && !plan.derivations.is_empty() {
            let mut command = nix::command("nix");
            command
                .args(["copy", "--no-check-sigs", "--to", &self.cache_store])
                .args(plan.derivations.iter().map(|d| d.drv.as_str()));
            nix::output(command)
                .await
                .wrap_err("pushing derivations to the cache")?;
        }
        Ok(plan)
    }

    pub async fn build(&self, job: Job) -> BuildResult {
        let key = Roots::job_key(&job.drv);
        self.roots.protect(
            &key,
            std::iter::once(&job.drv)
                .chain(&job.fetch)
                .chain(&job.outputs),
        );
        let result = match self.realise(&job).await {
            Ok(()) => BuildResult::Succeeded,
            Err(error) => BuildResult::Failed {
                error: format!("{error:#}"),
            },
        };
        self.roots.release(&key);
        result
    }

    async fn realise(&self, job: &Job) -> Result<()> {
        if !self.cache_host {
            let mut command = nix::command("nix");
            command
                .args([
                    "copy",
                    "--from",
                    &self.cache_url,
                    "--option",
                    "narinfo-cache-negative-ttl",
                    "0",
                    &job.drv,
                ])
                .args(&job.fetch);
            nix::output(command)
                .await
                .wrap_err("fetching the derivation and its inputs from the cache")?;
        }

        let drv = job.drv.clone();
        let mut command = nix::command("nix");
        command.args([
            "build",
            "--no-link",
            "--quiet",
            "--print-build-logs",
            "--max-jobs",
            "1",
            "--builders",
            "",
            &format!("{}^*", job.drv),
        ]);
        nix::run(command, &self.out, move |line| {
            Some(WorkerMessage::BuildLog {
                drv: drv.clone(),
                line,
            })
        })
        .await?;

        if !self.cache_host {
            let mut command = nix::command("nix");
            command
                .args(["copy", "--no-check-sigs", "--to", &self.cache_store])
                .args(&job.outputs);
            nix::output(command)
                .await
                .wrap_err("pushing outputs to the cache")?;
        }
        Ok(())
    }
}

async fn invalid_paths(paths: &[String]) -> Result<Vec<String>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut command = nix::command("nix-store");
    command
        .args(["--check-validity", "--print-invalid"])
        .args(paths);
    Ok(nix::output(command)
        .await?
        .lines()
        .map(str::to_owned)
        .collect())
}

async fn import(mut upload: mpsc::Receiver<Vec<u8>>) -> Result<()> {
    let mut child = nix::command("nix-store")
        .arg("--import")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .wrap_err("starting nix-store --import")?;
    if let Some(mut stdin) = child.stdin.take() {
        while let Some(chunk) = upload.recv().await {
            stdin.write_all(&chunk).await?;
        }
        stdin.shutdown().await?;
    }
    let status = child.wait().await?;
    if !status.success() {
        bail!("nix-store --import failed with {status}");
    }
    Ok(())
}

fn is_dry_run_summary(line: &str) -> bool {
    line.starts_with("  /nix/store/")
        || ((line.starts_with("these ") || line.starts_with("this "))
            && (line.ends_with(" will be built:") || line.contains(" will be fetched")))
}
