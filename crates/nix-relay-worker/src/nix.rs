use std::process::Stdio;

use color_eyre::eyre::{Result, WrapErr, bail};
use nix_relay_protocol::WorkerMessage;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
    sync::mpsc,
};

pub type Outbox = mpsc::Sender<WorkerMessage>;

pub fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    command.kill_on_drop(true).stdin(Stdio::null());
    command
}

pub async fn run<F>(mut command: Command, out: &Outbox, log: F) -> Result<Vec<u8>>
where
    F: Fn(String) -> Option<WorkerMessage> + Send + Sync,
{
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let description = describe(&command);
    let mut child = command
        .spawn()
        .wrap_err_with(|| format!("starting {description}"))?;
    let mut stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let read_stdout = async {
        let mut buf = Vec::new();
        if let Some(stdout) = &mut stdout {
            stdout.read_to_end(&mut buf).await?;
        }
        Ok::<_, std::io::Error>(buf)
    };
    let read_stderr = async {
        if let Some(stderr) = stderr {
            let mut lines = BufReader::new(stderr).lines();
            while let Some(line) = lines.next_line().await? {
                if let Some(message) = log(line)
                    && out.send(message).await.is_err()
                {
                    tracing::debug!("relay connection closed while streaming logs");
                }
            }
        }
        Ok::<_, std::io::Error>(())
    };
    let (stdout, ()) = tokio::try_join!(read_stdout, read_stderr)?;
    let status = child.wait().await?;
    if !status.success() {
        bail!("{description} failed with {status}");
    }
    Ok(stdout)
}

pub async fn output(mut command: Command) -> Result<String> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let description = describe(&command);
    let output = command
        .output()
        .await
        .wrap_err_with(|| format!("starting {description}"))?;
    if !output.status.success() {
        bail!(
            "{description} failed with {}:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub async fn config(name: &str) -> Result<String> {
    let mut command = self::command("nix");
    command.args(["config", "show", name]);
    Ok(output(command).await?.trim().to_owned())
}

fn describe(command: &Command) -> String {
    let command = command.as_std();
    let mut description = command.get_program().to_string_lossy().into_owned();
    if let Some(arg) = command.get_args().next() {
        description.push(' ');
        description.push_str(&arg.to_string_lossy());
    }
    description
}
