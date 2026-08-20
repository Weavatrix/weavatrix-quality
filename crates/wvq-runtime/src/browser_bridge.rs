//! Bounded Rust host for the bundled thin Playwright bridge.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use wvq_domain::ObligationId;

use crate::{Observation, TestAction, TestProgram};

const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 1024 * 1024;
const BRIDGE_FILES: [(&str, &str); 5] = [
    (
        "main.js",
        include_str!("../../../js/playwright-runner/dist/main.js"),
    ),
    (
        "protocol.js",
        include_str!("../../../js/playwright-runner/dist/protocol.js"),
    ),
    (
        "execute.js",
        include_str!("../../../js/playwright-runner/dist/execute.js"),
    ),
    (
        "observe.js",
        include_str!("../../../js/playwright-runner/dist/observe.js"),
    ),
    (
        "playwright.js",
        include_str!("../../../js/playwright-runner/dist/playwright.js"),
    ),
];

/// Fixed browser launch and repository paths supplied by local policy.
#[derive(Debug, Clone)]
pub struct BrowserRunConfig {
    /// Application origin.
    pub base_url: String,
    /// `chromium`, `firefox`, or `webkit`.
    pub browser: String,
    /// Headless launch.
    pub headless: bool,
    /// Per-action and whole bridge deadline.
    pub timeout: Duration,
    /// Repository-relative package root containing Playwright.
    pub module_root: PathBuf,
    /// Ignored local directory where the bundled bridge is materialized.
    pub runtime_dir: PathBuf,
    /// Ignored local directory for screenshot/trace files.
    pub evidence_dir: PathBuf,
    /// Cooperative cancellation.
    pub cancel: Arc<AtomicBool>,
}

/// Sealed oracle sent to the deterministic adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProgramOracle {
    /// Obligation identity.
    pub obligation: ObligationId,
    /// Optional sealed precondition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<Value>,
    /// Sealed expected predicate.
    pub expected: Value,
}

/// One complete browser-program result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserProgramRun {
    /// Program identity.
    pub program: String,
    /// All actions and sealed assertions passed.
    pub passed: bool,
    /// Obligations whose assertion executed successfully.
    pub asserted: Vec<String>,
    /// Obligations whose sealed expectation was contradicted.
    pub contradicted: Vec<String>,
    /// Structured observations after each attempted step.
    pub observations: Vec<Observation>,
    /// Screenshot files produced under [`BrowserRunConfig::evidence_dir`].
    pub screenshot_paths: Vec<PathBuf>,
    /// Optional trace file.
    pub trace_path: Option<PathBuf>,
    /// Stable failure text.
    pub failure: Option<String>,
}

/// Browser bridge startup, protocol, or evidence failure.
#[derive(Debug, Error)]
pub enum BrowserBridgeError {
    /// Invalid local configuration.
    #[error("invalid browser configuration: {0}")]
    Config(String),
    /// Filesystem/process I/O.
    #[error("browser bridge I/O: {0}")]
    Io(#[from] std::io::Error),
    /// Protocol mismatch or malformed reply.
    #[error("browser bridge protocol: {0}")]
    Protocol(String),
    /// Deadline exceeded.
    #[error("browser bridge deadline exceeded")]
    Deadline,
    /// Cooperative cancellation.
    #[error("browser bridge cancelled")]
    Cancelled,
    /// Bridge rejected a request.
    #[error("browser bridge: {0}")]
    Remote(String),
}

/// Execute one validated program through actual Playwright.
///
/// # Errors
///
/// Returns an error for startup/transport/protocol failures. A browser action
/// or sealed assertion failure is returned as a structured failed run.
pub fn run_browser_program(
    config: &BrowserRunConfig,
    program: &TestProgram,
    oracles: &[ProgramOracle],
) -> Result<BrowserProgramRun, BrowserBridgeError> {
    validate_config(config)?;
    program
        .validate()
        .map_err(|err| BrowserBridgeError::Config(err.to_string()))?;
    let known = oracles
        .iter()
        .map(|oracle| oracle.obligation.as_str())
        .collect::<BTreeSet<_>>();
    if let Some(missing) = program
        .obligations
        .iter()
        .find(|obligation| !known.contains(obligation.as_str()))
    {
        return Err(BrowserBridgeError::Config(format!(
            "program {} has no sealed predicate for {missing}",
            program.id
        )));
    }
    let runner = materialize_bridge(&config.runtime_dir)?;
    let mut bridge = BridgeProcess::spawn(config, &runner)?;
    bridge.request("initialize", &json!({"schema_v": 1}))?;
    bridge.request(
        "prepare",
        &json!({
            "program": program,
            "oracles": oracles,
            "config": {
                "base_url": config.base_url,
                "browser": config.browser,
                "headless": config.headless,
                "timeout_ms": u64::try_from(config.timeout.as_millis()).unwrap_or(u64::MAX),
                "evidence_dir": config.evidence_dir,
            }
        }),
    )?;

    let mut passed = true;
    let mut failure = None;
    let mut asserted = Vec::new();
    let mut contradicted = Vec::new();
    let mut observations = Vec::new();
    let mut screenshot_paths = Vec::new();
    for (index, step) in program.steps.iter().enumerate() {
        let step_result = bridge.request("execute_step", &json!({"index": index}));
        if let TestAction::Assert { obligation } = step {
            match &step_result {
                Ok(_) => asserted.push(obligation.to_string()),
                Err(BrowserBridgeError::Remote(message))
                    if message.starts_with(&format!("assertion_failed:{obligation}:")) =>
                {
                    contradicted.push(obligation.to_string());
                }
                _ => {}
            }
        }
        if let Err(err) = step_result {
            match err {
                BrowserBridgeError::Remote(message) => {
                    passed = false;
                    failure = Some(message);
                }
                other => return Err(other),
            }
        }
        let body = bridge.request("observe", &json!({"failed": !passed}))?;
        if let Some(path) = body.get("screenshot_path").and_then(Value::as_str) {
            screenshot_paths.push(validated_evidence_path(&config.evidence_dir, path)?);
        }
        observations.push(
            serde_json::from_value(body)
                .map_err(|err| BrowserBridgeError::Protocol(err.to_string()))?,
        );
        if !passed {
            break;
        }
    }
    let finished = bridge.request("finish", &json!({}))?;
    let trace_path = finished
        .get("trace_path")
        .and_then(Value::as_str)
        .map(|path| validated_evidence_path(&config.evidence_dir, path))
        .transpose()?;
    bridge.close()?;
    screenshot_paths.sort();
    screenshot_paths.dedup();
    Ok(BrowserProgramRun {
        program: program.id.to_string(),
        passed,
        asserted,
        contradicted,
        observations,
        screenshot_paths,
        trace_path,
        failure,
    })
}

fn validate_config(config: &BrowserRunConfig) -> Result<(), BrowserBridgeError> {
    if !config.base_url.starts_with("http://") && !config.base_url.starts_with("https://") {
        return Err(BrowserBridgeError::Config(
            "base_url must use http or https".into(),
        ));
    }
    if !matches!(config.browser.as_str(), "chromium" | "firefox" | "webkit") {
        return Err(BrowserBridgeError::Config(format!(
            "unknown browser `{}`",
            config.browser
        )));
    }
    if config.timeout.is_zero() || config.timeout > Duration::from_secs(120) {
        return Err(BrowserBridgeError::Config(
            "timeout must be between 1ms and 120s".into(),
        ));
    }
    if !config.module_root.is_dir() {
        return Err(BrowserBridgeError::Config(format!(
            "Playwright module root does not exist: {}",
            config.module_root.display()
        )));
    }
    Ok(())
}

fn materialize_bridge(runtime_dir: &Path) -> Result<PathBuf, BrowserBridgeError> {
    std::fs::create_dir_all(runtime_dir)?;
    write_if_changed(
        &runtime_dir.join("package.json"),
        b"{\"type\":\"module\"}\n",
    )?;
    for (name, contents) in BRIDGE_FILES {
        write_if_changed(&runtime_dir.join(name), contents.as_bytes())?;
    }
    Ok(runtime_dir.join("main.js"))
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    if std::fs::read(path).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    std::fs::write(path, bytes)
}

fn validated_evidence_path(root: &Path, raw: &str) -> Result<PathBuf, BrowserBridgeError> {
    let path = PathBuf::from(raw);
    let root = root.canonicalize()?;
    let path = path.canonicalize()?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(BrowserBridgeError::Protocol(format!(
            "bridge evidence escaped the configured directory: {}",
            path.display()
        )));
    }
    Ok(path)
}

struct BridgeProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    replies: mpsc::Receiver<Result<String, String>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    deadline: Instant,
    cancel: Arc<AtomicBool>,
    next_id: u64,
}

impl BridgeProcess {
    fn spawn(config: &BrowserRunConfig, runner: &Path) -> Result<Self, BrowserBridgeError> {
        let mut child = Command::new(if cfg!(windows) { "node.exe" } else { "node" })
            .arg(runner)
            .current_dir(&config.module_root)
            .env("WVQ_PLAYWRIGHT_MODULE_ROOT", &config.module_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| BrowserBridgeError::Protocol("child stdin was not piped".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| BrowserBridgeError::Protocol("child stdout was not piped".into()))?;
        let stderr_pipe = child
            .stderr
            .take()
            .ok_or_else(|| BrowserBridgeError::Protocol("child stderr was not piped".into()))?;
        let (sender, replies) = mpsc::sync_channel(4);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) if line.len() <= MAX_LINE_BYTES => {
                        if sender.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {
                        let _ = sender.send(Err("reply exceeded 8 MiB".into()));
                        break;
                    }
                    Err(err) => {
                        let _ = sender.send(Err(err.to_string()));
                        break;
                    }
                }
            }
        });
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let stderr_out = Arc::clone(&stderr);
        thread::spawn(move || {
            let mut reader = stderr_pipe.take(u64::try_from(MAX_STDERR_BYTES).unwrap_or(u64::MAX));
            let mut bytes = Vec::new();
            let _ = reader.read_to_end(&mut bytes);
            *stderr_out
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = bytes;
        });
        Ok(Self {
            child,
            stdin: Some(stdin),
            replies,
            stderr,
            deadline: Instant::now() + config.timeout,
            cancel: Arc::clone(&config.cancel),
            next_id: 1,
        })
    }

    fn request(&mut self, method: &str, params: &Value) -> Result<Value, BrowserBridgeError> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(BrowserBridgeError::Cancelled);
        }
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let mut line = serde_json::to_vec(&json!({
            "method": method,
            "id": id,
            "params": params,
        }))
        .map_err(|err| BrowserBridgeError::Protocol(err.to_string()))?;
        if line.len() > MAX_LINE_BYTES {
            return Err(BrowserBridgeError::Protocol(
                "request exceeded 8 MiB".into(),
            ));
        }
        line.push(b'\n');
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| BrowserBridgeError::Protocol("bridge stdin is closed".into()))?;
        stdin.write_all(&line)?;
        stdin.flush()?;
        let raw = loop {
            if self.cancel.load(Ordering::Acquire) {
                return Err(BrowserBridgeError::Cancelled);
            }
            let now = Instant::now();
            if now >= self.deadline {
                return Err(BrowserBridgeError::Deadline);
            }
            let wait = (self.deadline - now).min(Duration::from_millis(100));
            match self.replies.recv_timeout(wait) {
                Ok(Ok(line)) => break line,
                Ok(Err(message)) => return Err(BrowserBridgeError::Protocol(message)),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(BrowserBridgeError::Protocol(self.exit_detail()));
                }
            }
        };
        let reply: BridgeWireReply = serde_json::from_str(raw.trim_end())
            .map_err(|err| BrowserBridgeError::Protocol(err.to_string()))?;
        match reply {
            BridgeWireReply::Ok { id: found, body } if found == id => Ok(body),
            BridgeWireReply::Error { id: found, error } if found == id => {
                Err(BrowserBridgeError::Remote(error))
            }
            other => Err(BrowserBridgeError::Protocol(format!(
                "reply id mismatch: expected {id}, got {}",
                other.id()
            ))),
        }
    }

    fn close(&mut self) -> Result<(), BrowserBridgeError> {
        self.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if self.child.try_wait()?.is_some() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                self.child.kill()?;
                let _ = self.child.wait();
                return Err(BrowserBridgeError::Protocol(
                    "bridge did not exit after finish".into(),
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn exit_detail(&mut self) -> String {
        let status = self.child.try_wait().ok().flatten();
        let stderr = self
            .stderr
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        format!(
            "bridge output closed (status {status:?}): {}",
            String::from_utf8_lossy(&stderr)
        )
    }
}

impl Drop for BridgeProcess {
    fn drop(&mut self) {
        self.stdin.take();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BridgeWireReply {
    Ok {
        id: u64,
        #[serde(default)]
        body: Value,
    },
    Error {
        id: u64,
        error: String,
    },
}

impl BridgeWireReply {
    fn id(&self) -> u64 {
        match self {
            Self::Ok { id, .. } | Self::Error { id, .. } => *id,
        }
    }
}
