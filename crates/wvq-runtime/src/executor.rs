//! Registered executors. The program argv is never taken from MCP/user fields.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::normalize::RuntimeError;
use crate::process::{self, ProcessLimits, RawExecution};

/// Frozen identity of a runner (`vitest`, `go-test`, …).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExecutorId(String);

impl ExecutorId {
    /// Parse a non-empty executor id. Whitespace is rejected.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::InvalidArg`] when empty or containing whitespace.
    pub fn new(raw: impl AsRef<str>) -> Result<Self, RuntimeError> {
        let raw = raw.as_ref();
        if raw.is_empty() || raw.chars().any(char::is_whitespace) {
            return Err(RuntimeError::InvalidArg(
                "executor id must be a non-empty token".into(),
            ));
        }
        Ok(Self(raw.to_owned()))
    }

    /// Borrow the id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What a registered executor can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutorCapabilities {
    /// Produces JUnit-like case lists.
    pub cases: bool,
    /// Can emit LCOV or equivalent coverage.
    pub coverage: bool,
}

/// How to invoke a registered program. `program` is never user-supplied.
#[derive(Debug, Clone)]
pub struct ExecutorSpec {
    /// Registry key.
    pub id: ExecutorId,
    /// PATH executable name. Must not contain a path separator.
    pub program: String,
    /// Frozen argv prefix after the program.
    pub prefix: Vec<String>,
    /// Optional flag inserted once before the typed filter argv slots.
    pub filter_flag: Option<String>,
    /// Capabilities.
    pub capabilities: ExecutorCapabilities,
}

/// Request to prepare a registered run. Unknown map keys fail closed.
#[derive(Debug, Clone)]
pub struct PrepareRequest {
    /// Must match a registered id.
    pub executor: ExecutorId,
    /// Working directory.
    pub cwd: PathBuf,
    /// Optional test filters (separate argv values, never a shell string).
    pub filters: Vec<String>,
    /// Extra MCP/user fields. Only empty is accepted.
    pub extra: BTreeMap<String, String>,
    /// Deadline / output caps.
    pub limits: ProcessLimits,
    /// Cooperative cancel flag.
    pub cancel: Arc<AtomicBool>,
}

/// Frozen argv ready to spawn.
#[derive(Debug, Clone)]
pub struct PreparedRun {
    /// Executor that produced this argv.
    pub executor: ExecutorId,
    /// Registered program name.
    pub program: String,
    /// Arguments after the program. Never a user executable.
    pub args: Vec<String>,
    /// Working directory.
    pub cwd: PathBuf,
    /// Limits copied from the request.
    pub limits: ProcessLimits,
    /// Cancel flag.
    pub cancel: Arc<AtomicBool>,
}

/// Outcome of a bounded spawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    /// Process exit code, if it exited.
    pub status_code: Option<i32>,
    /// Capped stdout.
    pub stdout: Vec<u8>,
    /// Capped stderr.
    pub stderr: Vec<u8>,
}

/// Registry of allowed runners.
#[derive(Debug, Clone)]
pub struct ExecutorRegistry {
    specs: BTreeMap<ExecutorId, ExecutorSpec>,
}

impl ExecutorRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            specs: BTreeMap::new(),
        }
    }

    /// Vitest / Jest / Bun / Go / Playwright, frozen argv prefixes.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::InvalidArg`] if a built-in id is illegal.
    pub fn production() -> Result<Self, RuntimeError> {
        let mut registry = Self::new();
        registry.register(spec(
            "cargo-test",
            "cargo",
            &["test", "--workspace", "--all-targets"],
            None,
        )?)?;
        registry.register(spec(
            "npm-test",
            if cfg!(windows) { "npm.cmd" } else { "npm" },
            &["test", "--"],
            None,
        )?)?;
        registry.register(spec("vitest", "vitest", &["run"], None)?)?;
        registry.register(spec(
            "jest",
            "jest",
            &["--runInBand"],
            Some("--runTestsByPath"),
        )?)?;
        registry.register(spec("bun-test", "bun", &["test"], None)?)?;
        registry.register(spec(
            "go-test",
            "go",
            &[
                "test",
                "-json",
                "-coverprofile=.weavatrix-quality/go-cover.out",
                "./...",
            ],
            Some("-run"),
        )?)?;
        registry.register(spec("playwright", "playwright", &["test"], None)?)?;
        Ok(registry)
    }

    /// Add a spec. `program` must be a bare filename.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::InvalidArg`] when `program` looks like a path.
    pub fn register(&mut self, spec: ExecutorSpec) -> Result<(), RuntimeError> {
        validate_program(&spec.program)?;
        self.specs.insert(spec.id.clone(), spec);
        Ok(())
    }

    /// Build argv from typed fields only.
    ///
    /// # Errors
    ///
    /// Unknown executor, extra MCP keys, or unsafe filter.
    pub fn prepare(&self, request: PrepareRequest) -> Result<PreparedRun, RuntimeError> {
        reject_injected_command(&request.extra)?;
        if !request.extra.is_empty() {
            let keys = request.extra.keys().cloned().collect::<Vec<_>>();
            return Err(RuntimeError::InvalidArg(format!(
                "unknown executor fields: {}",
                keys.join(", ")
            )));
        }
        let spec = self
            .specs
            .get(&request.executor)
            .ok_or_else(|| RuntimeError::UnknownExecutor(request.executor.as_str().to_owned()))?;
        let mut args = spec.prefix.clone();
        if !request.filters.is_empty() {
            if let Some(flag) = &spec.filter_flag {
                args.push(flag.clone());
            }
            for filter in &request.filters {
                args.push(sanitize_filter(filter)?);
            }
        }
        Ok(PreparedRun {
            executor: spec.id.clone(),
            program: spec.program.clone(),
            args,
            cwd: request.cwd,
            limits: request.limits,
            cancel: request.cancel,
        })
    }

    /// Spawn the prepared argv with deadline, output cap, and cancel.
    ///
    /// # Errors
    ///
    /// Spawn, deadline, output limit, or cancel.
    pub fn execute(&self, run: &PreparedRun) -> Result<ExecutionResult, RuntimeError> {
        if run.cancel.load(Ordering::SeqCst) {
            return Err(RuntimeError::Cancelled);
        }
        let raw: RawExecution =
            process::run_bounded(&run.program, &run.args, &run.cwd, &run.limits, &run.cancel)?;
        Ok(ExecutionResult {
            status_code: raw.status_code,
            stdout: raw.stdout,
            stderr: raw.stderr,
        })
    }
}

impl Default for ExecutorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait from spec §15. Implemented by [`ExecutorRegistry`] via prepare/execute.
pub trait Executor {
    /// Declared capabilities of `id`, if registered.
    fn capabilities(&self, id: &ExecutorId) -> Option<ExecutorCapabilities>;
    /// Bind typed args to a frozen argv.
    ///
    /// # Errors
    ///
    /// Unknown id or illegal args.
    fn prepare(&self, request: PrepareRequest) -> Result<PreparedRun, RuntimeError>;
    /// Run a prepared argv.
    ///
    /// # Errors
    ///
    /// Process limit or spawn failure.
    fn execute(&self, run: &PreparedRun) -> Result<ExecutionResult, RuntimeError>;
}

impl Executor for ExecutorRegistry {
    fn capabilities(&self, id: &ExecutorId) -> Option<ExecutorCapabilities> {
        self.specs.get(id).map(|spec| spec.capabilities)
    }

    fn prepare(&self, request: PrepareRequest) -> Result<PreparedRun, RuntimeError> {
        Self::prepare(self, request)
    }

    fn execute(&self, run: &PreparedRun) -> Result<ExecutionResult, RuntimeError> {
        Self::execute(self, run)
    }
}

fn spec(
    id: &str,
    program: &str,
    prefix: &[&str],
    filter_flag: Option<&str>,
) -> Result<ExecutorSpec, RuntimeError> {
    Ok(ExecutorSpec {
        id: ExecutorId::new(id)?,
        program: program.to_owned(),
        prefix: prefix.iter().map(|item| (*item).to_owned()).collect(),
        filter_flag: filter_flag.map(ToOwned::to_owned),
        capabilities: ExecutorCapabilities {
            cases: true,
            coverage: matches!(id, "vitest" | "jest" | "bun-test" | "go-test"),
        },
    })
}

fn validate_program(program: &str) -> Result<(), RuntimeError> {
    if program.is_empty()
        || program.contains('/')
        || program.contains('\\')
        || program.contains("..")
    {
        return Err(RuntimeError::InvalidArg(
            "executor program must be a bare filename".into(),
        ));
    }
    Ok(())
}

fn reject_injected_command(extra: &BTreeMap<String, String>) -> Result<(), RuntimeError> {
    const FORBIDDEN: &[&str] = &[
        "command",
        "cmd",
        "shell",
        "argv",
        "executable",
        "program",
        "bin",
        "script",
    ];
    let forbidden = FORBIDDEN.iter().copied().collect::<BTreeSet<_>>();
    for key in extra.keys() {
        if forbidden.contains(key.as_str()) {
            return Err(RuntimeError::InvalidArg(format!(
                "field `{key}` cannot select an executable"
            )));
        }
    }
    Ok(())
}

fn sanitize_filter(filter: &str) -> Result<String, RuntimeError> {
    if filter.is_empty()
        || filter.contains('\0')
        || filter
            .chars()
            .any(|ch| matches!(ch, '\n' | '\r' | '|' | '&' | ';' | '`'))
    {
        return Err(RuntimeError::InvalidArg(
            "filter must be a single argv value without shell metacharacters".into(),
        ));
    }
    Ok(filter.to_owned())
}

/// Convenience constructor for tests and callers.
#[must_use]
pub fn default_limits() -> ProcessLimits {
    ProcessLimits {
        deadline: Duration::from_secs(900),
        max_output_bytes: 8 * 1024 * 1024,
    }
}
