use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;

pub type BuildId = u64;
pub type StorePath = String;
pub type DrvPath = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BuildRequest {
    Flake {
        source: StorePath,
        source_info: serde_json::Map<String, serde_json::Value>,
        subdir: String,
        attributes: Vec<Vec<String>>,
        sources: Vec<StorePath>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Submit { version: u32, request: BuildRequest },
    UploadDone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientEvent {
    Evaluating { worker: String },
    Upload { paths: Vec<StorePath> },
    Planned { derivations: usize },
    Started { drv: DrvPath, worker: String },
    Log { drv: Option<DrvPath>, line: String },
    Built { drv: DrvPath },
    Finished { outcome: Outcome },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Outcome {
    Succeeded {
        outputs: Vec<StorePath>,
        fetch_from: Option<String>,
    },
    Failed {
        error: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerInfo {
    pub name: String,
    pub systems: Vec<String>,
    pub features: Vec<String>,
    pub jobs: usize,
    pub cache_host: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerMessage {
    Register {
        version: u32,
        info: WorkerInfo,
    },
    Upload {
        build: BuildId,
        paths: Vec<StorePath>,
    },
    EvalLog {
        build: BuildId,
        line: String,
    },
    Evaluated {
        build: BuildId,
        result: EvalResult,
    },
    BuildLog {
        drv: DrvPath,
        line: String,
    },
    Built {
        drv: DrvPath,
        result: BuildResult,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayMessage {
    Welcome {
        cache_url: String,
        cache_store: String,
    },
    Eval {
        build: BuildId,
        request: BuildRequest,
    },
    UploadDone {
        build: BuildId,
    },
    Build {
        job: Job,
    },
    Protect {
        build: BuildId,
        paths: Vec<StorePath>,
    },
    Release {
        build: BuildId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub drv: DrvPath,
    pub outputs: Vec<StorePath>,
    pub fetch: Vec<StorePath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EvalResult {
    Planned { plan: Plan },
    Failed { error: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BuildResult {
    Succeeded,
    Failed { error: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub outputs: Vec<StorePath>,
    pub derivations: Vec<PlannedDerivation>,
    pub available: Vec<StorePath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedDerivation {
    pub drv: DrvPath,
    pub system: String,
    pub features: Vec<String>,
    pub prefer_local: bool,
    pub outputs: Vec<StorePath>,
    pub inputs: Vec<DrvPath>,
    pub fetch: Vec<StorePath>,
}

pub fn encode_frame(build: BuildId, data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(8 + data.len());
    frame.extend_from_slice(&build.to_be_bytes());
    frame.extend_from_slice(data);
    frame
}

pub fn decode_frame(frame: &[u8]) -> Option<(BuildId, &[u8])> {
    let (id, data) = frame.split_first_chunk::<8>()?;
    Some((BuildId::from_be_bytes(*id), data))
}

pub fn store_path_name(path: &str) -> &str {
    let base = path.rsplit('/').next().unwrap_or(path);
    let name = base.split_once('-').map_or(base, |(_, name)| name);
    name.strip_suffix(".drv").unwrap_or(name)
}

pub fn nix_string(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            '$' if chars.peek() == Some(&'{') => quoted.push_str("\\$"),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests;
