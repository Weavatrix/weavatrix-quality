//! Domain facade. CLI and MCP call this; they do not reimplement policy.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
use wvq_domain::{ArtifactId, ProofId, RevisionId, RunId};
use wvq_intelligence::{
    CodeEvidenceProvider, CoverageMeasurement, GraphDelta, ObligationNeed, SelectionInput,
    SurfaceDelta, TestCandidate, WeavatrixProvider, impacted_surface, map_coverage_to_nodes,
    select_minimal_plan,
};
use wvq_proof::{
    AiBudget, AiCallKind, AiCostFirewall, AiUsage, AssemblyInput, ExecutionEvidence,
    FlowProtection, LocalModelConfig, LocalModelRequest, ProofVerdict, assemble, call_local_model,
    snapshot,
};
use wvq_runtime::{
    CoverageArtifact, ExecutionResult, ExecutorRegistry, ExecutorTarget, PrepareRequest,
    default_limits, discover_executor_targets, parse_go_coverprofile, parse_go_json, parse_junit,
    parse_lcov,
};
use wvq_spec::{
    EvidenceKind, ObligationKind, OpenSpecChange, RequirementOp, RiskLevel, SpecError,
    TestObligation, compile_obligations, load_quality_contract, read_change, seal,
};
use wvq_store::{Store, StoredAiUsage, StoredProof, StoredRun, StoredRunItem};

use crate::commands::{
    ChangesCommand, Command, ContextCommand, DebtCommand, EvidenceCommand, ExplainCommand,
    ModelCommand, PlanCommand, RunCommand, SelectCommand, SpecCommand, StatusCommand,
    VerifyCommand,
};
use crate::replies::{
    ChangesReply, ContextReply, DebtReply, EvidenceReply, ExplainReply, INLINE_LIMIT, ModelReply,
    PlanReply, ProofSummary, Reply, RunReply, SelectReply, SpecSealReply, SpecValidateReply,
    StatusReply, VerifyReply, bound_items,
};

/// Command-bus failure. Unknown values fail closed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BusError {
    /// `OpenSpec` / `quality.yaml` error.
    #[error(transparent)]
    Spec(#[from] SpecError),
    /// Requested entity does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// `current` matched more than one change.
    #[error("{0}")]
    Ambiguous(String),
    /// Unknown enum / policy token.
    #[error("unknown {field} `{value}`")]
    Unknown {
        /// Field name.
        field: &'static str,
        /// Rejected token.
        value: String,
    },
    /// Identity or revision could not be formed.
    #[error("invalid identity: {0}")]
    Identity(String),
    /// Registered runner discovery, preparation, or execution failed.
    #[error("runtime: {0}")]
    Runtime(String),
    /// Revision-bound Weavatrix evidence failed.
    #[error("intelligence: {0}")]
    Intelligence(String),
    /// Evidence ledger or CAS failed.
    #[error("store: {0}")]
    Store(String),
    /// Explicit loopback model call or AI Cost Firewall failed.
    #[error("model: {0}")]
    Model(String),
}

/// Shared facade for every host.
pub trait QualityService: Send + Sync {
    /// Bounded neighbouring context.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when the change cannot be resolved or the purpose is unknown.
    fn context(&self, cmd: &ContextCommand) -> Result<ContextReply, BusError>;
    /// Deterministic plan. Must not execute runners.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when the change cannot be compiled.
    fn plan(&self, cmd: &PlanCommand) -> Result<PlanReply, BusError>;
    /// Execute a bounded run through a registered executor producer. No arbitrary shell.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] on unknown scope/policy, missing change, or an unavailable executor
    /// producer.
    fn run(&self, cmd: &RunCommand) -> Result<RunReply, BusError>;
    /// Execute with a cooperative cancellation flag supplied by a transport.
    ///
    /// The default preserves compatibility for embedded services. Live
    /// execution overrides this and passes the flag into every bounded process.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`QualityService::run`].
    fn run_controlled(
        &self,
        cmd: &RunCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RunReply, BusError> {
        let _ = cancel;
        self.run(cmd)
    }
    /// Compact run progress and handles.
    ///
    /// # Errors
    ///
    /// Returns [`BusError::NotFound`] when a requested run is unknown.
    fn status(&self, cmd: &StatusCommand) -> Result<StatusReply, BusError>;
    /// Assemble revision-bound proofs.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when the change cannot be compiled.
    fn verify(&self, cmd: &VerifyCommand) -> Result<VerifyReply, BusError>;
    /// Explain one identity with provenance.
    ///
    /// # Errors
    ///
    /// Returns [`BusError::NotFound`] when the id is unknown.
    fn explain(&self, cmd: &ExplainCommand) -> Result<ExplainReply, BusError>;
    /// Evidence metadata; large bodies remain handles.
    ///
    /// # Errors
    ///
    /// Returns [`BusError::NotFound`] when the handle is unknown.
    fn evidence(&self, cmd: &EvidenceCommand) -> Result<EvidenceReply, BusError>;
    /// Validate `OpenSpec` + contract.
    ///
    /// # Errors
    ///
    /// Returns [`BusError::Spec`] on malformed intent.
    fn spec_validate(&self, cmd: &SpecCommand) -> Result<SpecValidateReply, BusError>;
    /// Seal compiled obligations.
    ///
    /// # Errors
    ///
    /// Returns [`BusError::Spec`] on malformed intent.
    fn spec_seal(&self, cmd: &SpecCommand) -> Result<SpecSealReply, BusError>;
    /// Debt-ratchet buckets.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when the change cannot be resolved.
    fn debt(&self, cmd: &DebtCommand) -> Result<DebtReply, BusError>;
    /// Minimal impacted selection. Does not execute.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when the change cannot be compiled.
    fn select(&self, cmd: &SelectCommand) -> Result<SelectReply, BusError>;
    /// Execute one explicit loopback model call through the per-change AI Cost Firewall.
    ///
    /// Normal verification never invokes this method.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when model policy, transport, usage evidence, or budget is invalid.
    fn model(&self, cmd: &ModelCommand) -> Result<ModelReply, BusError>;
    /// Known `OpenSpec` changes.
    ///
    /// # Errors
    ///
    /// Returns [`BusError::NotFound`] when `openspec/changes` cannot be read.
    fn changes(&self, cmd: &ChangesCommand) -> Result<ChangesReply, BusError>;
}

/// Dispatch a [`Command`] through any [`QualityService`].
///
/// # Errors
///
/// Propagates [`BusError`] from the service method.
pub fn dispatch(service: &dyn QualityService, command: Command) -> Result<Reply, BusError> {
    match command {
        Command::Context(cmd) => service.context(&cmd).map(Reply::Context),
        Command::Analyze(cmd) => service.context(&cmd).map(Reply::Analyze),
        Command::Plan(cmd) => service.plan(&cmd).map(Reply::Plan),
        Command::Run(cmd) => service.run(&cmd).map(Reply::Run),
        Command::Status(cmd) => service.status(&cmd).map(Reply::Status),
        Command::Verify(cmd) => service.verify(&cmd).map(Reply::Verify),
        Command::Explain(cmd) => service.explain(&cmd).map(Reply::Explain),
        Command::Evidence(cmd) => service.evidence(&cmd).map(Reply::Evidence),
        Command::SpecValidate(cmd) => service.spec_validate(&cmd).map(Reply::SpecValidate),
        Command::SpecSeal(cmd) => service.spec_seal(&cmd).map(Reply::SpecSeal),
        Command::Debt(cmd) => service.debt(&cmd).map(Reply::Debt),
        Command::Select(cmd) => service.select(&cmd).map(Reply::Select),
        Command::Model(cmd) => service.model(&cmd).map(Reply::Model),
        Command::Changes(cmd) => service.changes(&cmd).map(Reply::Changes),
    }
}

/// In-memory provider for tests. No filesystem, no subprocesses.
#[derive(Debug)]
pub struct FakeService {
    inner: Mutex<FakeInner>,
}

#[derive(Debug)]
struct FakeInner {
    context_items: Vec<String>,
    verdict: String,
    evidence: BTreeMap<String, Vec<u8>>,
    run_executed: bool,
    last_run: Option<RunState>,
    explanations: BTreeMap<String, ExplainReply>,
    proofs: Vec<ProofSummary>,
}

#[derive(Debug, Clone)]
struct RunState {
    id: String,
    status: String,
    outcome: String,
    handles: Vec<String>,
}

impl Default for FakeService {
    fn default() -> Self {
        Self {
            inner: Mutex::new(FakeInner {
                context_items: vec![
                    "requirement: others remain visible".into(),
                    "obligation: others-visible".into(),
                    "heuristic: 0 runtime LLM tokens".into(),
                    "coverage: unmeasured is not uncovered".into(),
                ],
                verdict: "UNPROVEN".into(),
                evidence: BTreeMap::new(),
                run_executed: false,
                last_run: None,
                explanations: BTreeMap::new(),
                proofs: Vec::new(),
            }),
        }
    }
}

impl FakeService {
    /// Programmable overall verify verdict (`PROVEN`, `CONTRADICTED`, …).
    pub fn set_verdict(&self, verdict: impl Into<String>) {
        self.lock().verdict = verdict.into();
    }

    /// Items that [`QualityService::context`] will bound.
    pub fn set_context_items(&self, items: Vec<String>) {
        self.lock().context_items = items;
    }

    /// Store a blob behind a handle.
    pub fn put_evidence(&self, handle: impl Into<String>, bytes: Vec<u8>) {
        self.lock().evidence.insert(handle.into(), bytes);
    }

    /// Register an explanation.
    pub fn put_explain(&self, reply: ExplainReply) {
        self.lock().explanations.insert(reply.id.clone(), reply);
    }

    /// Per-obligation proofs [`QualityService::verify`] should return.
    ///
    /// Empty (the default) keeps the single placeholder proof.
    pub fn set_proofs(&self, proofs: Vec<ProofSummary>) {
        self.lock().proofs = proofs;
    }

    /// Whether [`QualityService::run`] was invoked.
    #[must_use]
    pub fn run_was_executed(&self) -> bool {
        self.lock().run_executed
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FakeInner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl QualityService for FakeService {
    fn context(&self, cmd: &ContextCommand) -> Result<ContextReply, BusError> {
        validate_purpose(&cmd.purpose)?;
        let items = self.lock().context_items.clone();
        Ok(pack_context(
            &cmd.change,
            &cmd.purpose,
            cmd.token_budget,
            items,
        ))
    }

    fn plan(&self, cmd: &PlanCommand) -> Result<PlanReply, BusError> {
        Ok(PlanReply {
            change: cmd.change.clone(),
            requirements: vec!["sankey.visual-limit-others".into()],
            obligations: vec!["others-visible".into()],
            risk: vec!["requirement_criticality high sankey.visual-limit-others".into()],
            existing_proofs: Vec::new(),
            gaps: vec!["others-visible: no runtime evidence".into()],
            checks: deterministic_checks(),
            executed: false,
        })
    }

    fn run(&self, cmd: &RunCommand) -> Result<RunReply, BusError> {
        self.run_controlled(cmd, Arc::new(AtomicBool::new(false)))
    }

    fn run_controlled(
        &self,
        cmd: &RunCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RunReply, BusError> {
        let _ = cancel;
        validate_scope(&cmd.scope)?;
        validate_evidence_policy(&cmd.evidence_policy)?;
        let mut inner = self.lock();
        inner.run_executed = true;
        let state = RunState {
            id: "run-fake".into(),
            status: "complete".into(),
            outcome: "passed".into(),
            handles: inner.evidence.keys().cloned().collect(),
        };
        inner.last_run = Some(state.clone());
        Ok(RunReply {
            run_id: state.id,
            change: cmd.change.clone(),
            base: cmd.base.clone(),
            head: cmd.head.clone(),
            requested_scope: cmd.scope.clone(),
            scope: cmd.scope.clone(),
            status: "complete".into(),
            executed: true,
            outcome: state.outcome,
            artifact_handles: state.handles,
        })
    }

    fn status(&self, cmd: &StatusCommand) -> Result<StatusReply, BusError> {
        let inner = self.lock();
        match (&cmd.run_id, &inner.last_run) {
            (Some(want), Some(run)) if want != &run.id => {
                Err(BusError::NotFound(format!("run {want}")))
            }
            (_, Some(run)) => Ok(StatusReply {
                run_id: Some(run.id.clone()),
                status: run.status.clone(),
                outcome: Some(run.outcome.clone()),
                handles: run.handles.clone(),
            }),
            (Some(want), None) => Err(BusError::NotFound(format!("run {want}"))),
            (None, None) => Ok(StatusReply {
                run_id: None,
                status: "idle".into(),
                outcome: None,
                handles: Vec::new(),
            }),
        }
    }

    fn verify(&self, cmd: &VerifyCommand) -> Result<VerifyReply, BusError> {
        let inner = self.lock();
        let verdict = inner.verdict.clone();
        let proofs = inner.proofs.clone();
        drop(inner);
        let mut reply = verify_from_token(&cmd.change, &verdict);
        if !proofs.is_empty() {
            reply.proofs = proofs;
        }
        Ok(reply)
    }

    fn explain(&self, cmd: &ExplainCommand) -> Result<ExplainReply, BusError> {
        let inner = self.lock();
        inner
            .explanations
            .get(&cmd.id)
            .cloned()
            .ok_or_else(|| BusError::NotFound(format!("id {}", cmd.id)))
    }

    fn evidence(&self, cmd: &EvidenceCommand) -> Result<EvidenceReply, BusError> {
        let inner = self.lock();
        let bytes = inner
            .evidence
            .get(&cmd.handle)
            .ok_or_else(|| BusError::NotFound(format!("handle {}", cmd.handle)))?;
        Ok(evidence_from_bytes(&cmd.handle, bytes))
    }

    fn spec_validate(&self, cmd: &SpecCommand) -> Result<SpecValidateReply, BusError> {
        Ok(SpecValidateReply {
            change: cmd.change.clone(),
            requirements: 1,
            obligations: 1,
            ok: true,
        })
    }

    fn spec_seal(&self, cmd: &SpecCommand) -> Result<SpecSealReply, BusError> {
        Ok(SpecSealReply {
            change: cmd.change.clone(),
            seal_id: "oseal-fake".into(),
            digest: "ab".repeat(32),
            obligations: 1,
        })
    }

    fn debt(&self, cmd: &DebtCommand) -> Result<DebtReply, BusError> {
        Ok(empty_debt(&cmd.base, &cmd.head))
    }

    fn select(&self, cmd: &SelectCommand) -> Result<SelectReply, BusError> {
        Ok(SelectReply {
            base: cmd.base.clone(),
            head: cmd.head.clone(),
            revision: None,
            algorithm: "greedy-weighted-set-cover".into(),
            selected: Vec::new(),
            uncovered_mandatory: vec!["others-visible".into()],
            explanations: Vec::new(),
            executed: false,
            selection_complete: false,
        })
    }

    fn model(&self, cmd: &ModelCommand) -> Result<ModelReply, BusError> {
        let kind = parse_model_kind(&cmd.kind)?;
        let (planning_tokens, runtime_tokens, browser_escape_calls, vision_calls) = match kind {
            AiCallKind::Planning => (1, 0, 0, 0),
            AiCallKind::Runtime => (0, 1, 0, 0),
            AiCallKind::BrowserEscape => (0, 1, 1, 0),
            AiCallKind::Vision => (0, 1, 0, 1),
        };
        Ok(ModelReply {
            change: cmd.change.clone(),
            kind: cmd.kind.clone(),
            model: "fake-local-model".into(),
            text: "fake model decision".into(),
            input_tokens: planning_tokens + runtime_tokens,
            output_tokens: 0,
            cost_micros: browser_escape_calls + vision_calls,
        })
    }

    fn changes(&self, cmd: &ChangesCommand) -> Result<ChangesReply, BusError> {
        let _ = cmd;
        Ok(ChangesReply {
            changes: vec!["sankey-others".into()],
        })
    }
}

/// Filesystem-backed service with registered bounded executors and a persistent evidence ledger.
#[derive(Debug)]
pub struct LiveService {
    repo: PathBuf,
    state: Mutex<Option<RunState>>,
    executors: ExecutorRegistry,
    executor_init_error: Option<String>,
}

impl LiveService {
    /// `repo` is the repository root that contains `openspec/`.
    #[must_use]
    pub fn new(repo: impl AsRef<Path>) -> Self {
        let (executors, executor_init_error) = match ExecutorRegistry::production() {
            Ok(registry) => (registry, None),
            Err(err) => (ExecutorRegistry::new(), Some(err.to_string())),
        };
        Self {
            repo: repo.as_ref().to_path_buf(),
            state: Mutex::new(None),
            executors,
            executor_init_error,
        }
    }

    /// Construct a live service with an explicit registered executor set.
    /// Intended for controlled embedding and integration tests.
    #[must_use]
    pub fn with_executors(repo: impl AsRef<Path>, executors: ExecutorRegistry) -> Self {
        Self {
            repo: repo.as_ref().to_path_buf(),
            state: Mutex::new(None),
            executors,
            executor_init_error: None,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<RunState>> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn compiled(&self, change: &str) -> Result<Compiled, BusError> {
        let change = resolve_change(&self.repo, change)?;
        let spec = read_change(&self.repo, &change)?;
        let contract = load_quality_contract(&self.repo, &change)?;
        let obligations = compile_obligations(&contract, &spec)?;
        Ok(Compiled {
            change,
            spec,
            obligations,
        })
    }

    fn store(&self) -> Result<Store, BusError> {
        Store::open(&self.repo).map_err(|err| BusError::Store(err.to_string()))
    }

    fn revision(&self) -> Result<RevisionId, BusError> {
        WeavatrixProvider
            .analyze(&self.repo)
            .map(|evidence| evidence.revision)
            .map_err(|err| BusError::Intelligence(err.to_string()))
    }

    fn require_git_root(&self) -> Result<(), BusError> {
        if self.repo.join(".git").exists() {
            Ok(())
        } else {
            Err(BusError::Intelligence(format!(
                "base/head analysis requires the repository Git root, got {}",
                self.repo.display()
            )))
        }
    }

    fn revision_range(&self, base: &str, head: &str) -> Result<RevisionRange, BusError> {
        self.require_git_root()?;
        validate_revision_ref("base", base)?;
        validate_revision_ref("head", head)?;

        let checked_out_head = self.resolve_commit("HEAD")?;
        let head_commit = if head == "WORKTREE" {
            checked_out_head.clone()
        } else {
            let requested_head = self.resolve_commit(head)?;
            if requested_head != checked_out_head {
                return Err(BusError::Ambiguous(format!(
                    "explicit head `{head}` resolves to `{requested_head}`, but the checked-out HEAD is `{checked_out_head}`"
                )));
            }
            if self.worktree_is_dirty()? {
                return Err(BusError::Ambiguous(format!(
                    "explicit committed head `{head}` requires a clean repository; dirty worktree content must use head `WORKTREE`"
                )));
            }
            requested_head
        };
        let base_commit = self.resolve_commit(base)?;
        Ok(RevisionRange {
            base_ref: base.to_owned(),
            base_commit,
            head_ref: head.to_owned(),
            head_commit,
        })
    }

    fn resolve_commit(&self, reference: &str) -> Result<String, BusError> {
        let output = ProcessCommand::new("git")
            .args(["rev-parse", "--verify", "--end-of-options"])
            .arg(format!("{reference}^{{commit}}"))
            .current_dir(&self.repo)
            .output()
            .map_err(|err| {
                BusError::Intelligence(format!("cannot resolve Git ref `{reference}`: {err}"))
            })?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(BusError::Intelligence(format!(
                "cannot resolve Git ref `{reference}` to a commit{}",
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(": {detail}")
                }
            )));
        }
        let commit = String::from_utf8(output.stdout).map_err(|err| {
            BusError::Intelligence(format!("Git returned a non-UTF-8 commit id: {err}"))
        })?;
        let commit = commit.trim();
        if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(BusError::Intelligence(format!(
                "Git returned an invalid commit id for `{reference}`: `{commit}`"
            )));
        }
        Ok(commit.to_owned())
    }

    fn worktree_is_dirty(&self) -> Result<bool, BusError> {
        let output = ProcessCommand::new("git")
            .args([
                "status",
                "--porcelain=v1",
                "--untracked-files=normal",
                "--ignore-submodules=none",
            ])
            .current_dir(&self.repo)
            .output()
            .map_err(|err| BusError::Intelligence(format!("cannot inspect Git worktree: {err}")))?;
        if !output.status.success() {
            return Err(BusError::Intelligence(format!(
                "cannot inspect Git worktree: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(!output.stdout.is_empty())
    }

    fn weavatrix_operation(
        &self,
        revision: &RevisionId,
        name: &str,
        args: &Value,
    ) -> Result<Value, BusError> {
        let report = WeavatrixProvider
            .operation(&self.repo, name, args)
            .map_err(|err| BusError::Intelligence(err.to_string()))?;
        let found = report
            .get("revision")
            .and_then(Value::as_str)
            .ok_or_else(|| BusError::Intelligence(format!("{name} omitted revision identity")))?;
        if found != revision.as_str() {
            return Err(BusError::Ambiguous(format!(
                "{name} evidence belongs to revision `{found}`, expected `{revision}`"
            )));
        }
        Ok(report)
    }
}

struct Compiled {
    change: String,
    spec: OpenSpecChange,
    obligations: Vec<TestObligation>,
}

struct RevisionRange {
    base_ref: String,
    base_commit: String,
    head_ref: String,
    head_commit: String,
}

#[derive(Debug)]
struct ExecutorRecord {
    executor: String,
    cwd: String,
    selection: Option<String>,
    status_code: Option<i32>,
    passed: bool,
    error: Option<String>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    artifacts: Vec<ProducedArtifact>,
}

#[derive(Debug)]
struct ProducedArtifact {
    kind: String,
    path: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
struct TestBinding {
    path: String,
    obligations: BTreeSet<String>,
    cost: u64,
    flake_penalty: u64,
}

struct ModelPolicy {
    model: LocalModelConfig,
    budget: AiBudget,
}

struct LiveSelection {
    selected: Vec<String>,
    explanations: Vec<Vec<String>>,
    uncovered_mandatory: Vec<String>,
    uncovered_all: Vec<String>,
    bindings: Vec<TestBinding>,
}

struct ExecutionRequest {
    target: ExecutorTarget,
    filter: Option<String>,
    selected_test: Option<String>,
}

impl LiveSelection {
    fn complete(&self) -> bool {
        self.uncovered_all.is_empty()
    }
}

impl QualityService for LiveService {
    fn context(&self, cmd: &ContextCommand) -> Result<ContextReply, BusError> {
        validate_purpose(&cmd.purpose)?;
        let compiled = self.compiled(&cmd.change)?;
        let mut items = requirement_texts(&compiled.spec);
        items.extend(obligation_texts(&compiled.obligations));
        items.push("heuristic: 0 runtime LLM tokens on the green path".into());
        items.push("coverage: unmeasured is not uncovered".into());
        Ok(pack_context(
            &compiled.change,
            &cmd.purpose,
            cmd.token_budget,
            items,
        ))
    }

    fn plan(&self, cmd: &PlanCommand) -> Result<PlanReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let requirements = unique_requirements(&compiled.obligations);
        let obligations = obligation_texts(&compiled.obligations);
        let risk = compiled
            .obligations
            .iter()
            .map(|item| format!("obligation {} risk {}", item.id, risk_token(item.risk)))
            .collect();
        let revision = self.revision()?;
        let store = self.store()?;
        let mut existing_proofs = Vec::new();
        let mut gaps = Vec::new();
        for obligation in &compiled.obligations {
            let proof = store
                .proof_for_obligation(&revision, &obligation.id)
                .map_err(|err| BusError::Store(err.to_string()))?;
            match proof {
                Some(proof) if proof.verdict == "PROVEN" => {
                    existing_proofs.push(proof.id.to_string());
                }
                Some(proof) => {
                    existing_proofs.push(proof.id.to_string());
                    gaps.push(format!(
                        "{}: proof verdict {}",
                        obligation.id, proof.verdict
                    ));
                }
                None => gaps.push(format!("{}: no same-revision proof", obligation.id)),
            }
        }
        Ok(PlanReply {
            change: compiled.change,
            requirements,
            obligations,
            risk,
            existing_proofs,
            gaps,
            checks: deterministic_checks(),
            executed: false,
        })
    }

    fn run(&self, cmd: &RunCommand) -> Result<RunReply, BusError> {
        self.run_controlled(cmd, Arc::new(AtomicBool::new(false)))
    }

    #[allow(clippy::too_many_lines)]
    fn run_controlled(
        &self,
        cmd: &RunCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RunReply, BusError> {
        validate_scope(&cmd.scope)?;
        validate_evidence_policy(&cmd.evidence_policy)?;
        let compiled = self.compiled(&cmd.change)?;
        if let Some(err) = &self.executor_init_error {
            return Err(BusError::Runtime(format!(
                "registered executor initialization failed: {err}"
            )));
        }
        let targets = discover_executor_targets(&self.repo)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        if targets.is_empty() {
            return Err(BusError::Runtime(
                "no supported registered executor was discovered from repository manifests".into(),
            ));
        }
        let range = self.revision_range(&cmd.base, &cmd.head)?;
        let store = self.store()?;
        let before = self.revision()?;
        let graph_diff = self.weavatrix_operation(
            &before,
            "graph_diff",
            &json!({
                "base_ref": range.base_ref,
                "detail": "edges",
                "max_results": 100_000
            }),
        )?;
        let change_impact = self.weavatrix_operation(
            &before,
            "change_impact",
            &json!({
                "base_ref": range.base_ref,
                "depth": 6,
                "max_nodes": 100_000,
                "precision": "graph"
            }),
        )?;
        let static_selection = self.weavatrix_operation(
            &before,
            "select_tests",
            &json!({
                "base_ref": range.base_ref,
                "depth": 6,
                "max_nodes": 2000,
                "max_tests": 500,
                "precision": "graph"
            }),
        )?;
        let obligation_needs: Vec<_> = compiled
            .obligations
            .iter()
            .map(|item| ObligationNeed {
                id: item.id.to_string(),
                high_risk: matches!(item.risk, RiskLevel::High | RiskLevel::Critical),
            })
            .collect();
        let live_selection = build_live_selection(
            &self.repo,
            &static_selection,
            &graph_diff,
            &obligation_needs,
        )?;
        let impact = live_impacted_surface(&graph_diff, &change_impact)?;
        let (execution_requests, effective_scope, executed_tests) =
            build_execution_requests(&self.repo, &targets, &live_selection, &cmd.scope);
        let mut records = Vec::new();
        for request in &execution_requests {
            let target = &request.target;
            std::fs::create_dir_all(target.cwd.join(".weavatrix-quality")).map_err(|err| {
                BusError::Runtime(format!(
                    "cannot prepare runner evidence directory in {}: {err}",
                    target.cwd.display()
                ))
            })?;
            let prepared = self
                .executors
                .prepare(PrepareRequest {
                    executor: target.executor.clone(),
                    cwd: target.cwd.clone(),
                    filter: request.filter.clone(),
                    extra: BTreeMap::new(),
                    limits: default_limits(),
                    cancel: Arc::clone(&cancel),
                })
                .map_err(|err| BusError::Runtime(err.to_string()))?;
            let started = SystemTime::now();
            let mut record = match self.executors.execute(&prepared) {
                Ok(ExecutionResult {
                    status_code,
                    stdout,
                    stderr,
                }) => ExecutorRecord {
                    executor: target.executor.as_str().to_owned(),
                    cwd: relative_or_display(&self.repo, &target.cwd),
                    selection: request.selected_test.clone(),
                    status_code,
                    passed: status_code == Some(0),
                    error: None,
                    stdout,
                    stderr,
                    artifacts: Vec::new(),
                },
                Err(err) => ExecutorRecord {
                    executor: target.executor.as_str().to_owned(),
                    cwd: relative_or_display(&self.repo, &target.cwd),
                    selection: request.selected_test.clone(),
                    status_code: None,
                    passed: false,
                    error: Some(err.to_string()),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    artifacts: Vec::new(),
                },
            };
            attach_normalized_artifacts(&self.repo, &target.cwd, started, &mut record);
            records.push(record);
        }

        let after = self.revision()?;
        if before != after {
            return Err(BusError::Ambiguous(format!(
                "repository revision changed during execution: `{before}` -> `{after}`"
            )));
        }
        let outcome = if records.iter().any(|record| record.error.is_some()) {
            "error"
        } else if records.iter().all(|record| record.passed) {
            "passed"
        } else {
            "failed"
        };
        let run_id = make_run_id(&compiled.change, &before)?;
        store
            .put_run(&StoredRun {
                id: run_id.clone(),
                change_id: compiled.change.clone(),
                revision: before.clone(),
                status: "complete".into(),
                passed: outcome == "passed",
                outcome: outcome.into(),
            })
            .map_err(|err| BusError::Store(err.to_string()))?;

        let mut handles = Vec::new();
        put_json_run_artifact(
            &store,
            &run_id,
            &format!("artifact-{}-revision-range", run_id.as_str()),
            "revision-range",
            &json!({
                "schema_v": 1,
                "base": {"ref": range.base_ref, "commit": range.base_commit},
                "head": {
                    "ref": range.head_ref,
                    "commit": range.head_commit,
                    "content_revision": before.as_str()
                }
            }),
            &mut handles,
        )?;
        put_json_run_artifact(
            &store,
            &run_id,
            &format!("artifact-{}-graph-diff", run_id.as_str()),
            "weavatrix-graph-diff",
            &graph_diff,
            &mut handles,
        )?;
        let obligation_execution = obligation_execution_map(
            &self.repo,
            &targets,
            &live_selection.bindings,
            executed_tests.as_ref(),
        );
        put_json_run_artifact(
            &store,
            &run_id,
            &format!("artifact-{}-obligation-execution", run_id.as_str()),
            "obligation-execution-map",
            &obligation_execution,
            &mut handles,
        )?;
        put_json_run_artifact(
            &store,
            &run_id,
            &format!("artifact-{}-impact", run_id.as_str()),
            "impacted-surface",
            &impact,
            &mut handles,
        )?;
        put_json_run_artifact(
            &store,
            &run_id,
            &format!("artifact-{}-selection", run_id.as_str()),
            "weavatrix-test-selection",
            &static_selection,
            &mut handles,
        )?;
        for (index, record) in records.iter().enumerate() {
            store
                .put_run_item(&StoredRunItem {
                    id: format!("{}-item-{index}", run_id.as_str()),
                    run_id: run_id.clone(),
                    executor: record.executor.clone(),
                    status_code: record.status_code,
                    passed: record.passed,
                })
                .map_err(|err| BusError::Store(err.to_string()))?;
            let keep_raw_streams = cmd.evidence_policy == "standard"
                || (cmd.evidence_policy == "minimal" && !record.passed);
            if keep_raw_streams && !record.stdout.is_empty() {
                put_run_artifact(
                    &store,
                    &run_id,
                    &format!("artifact-{}-{index}-stdout", run_id.as_str()),
                    stdout_kind(&record.executor),
                    &record.stdout,
                    &mut handles,
                )?;
            }
            if keep_raw_streams && !record.stderr.is_empty() {
                put_run_artifact(
                    &store,
                    &run_id,
                    &format!("artifact-{}-{index}-stderr", run_id.as_str()),
                    "stderr",
                    &record.stderr,
                    &mut handles,
                )?;
            }
            for (artifact_index, artifact) in record.artifacts.iter().enumerate() {
                let keep = cmd.evidence_policy == "standard"
                    || (cmd.evidence_policy == "minimal"
                        && matches!(artifact.kind.as_str(), "normalized-test-run" | "coverage"));
                if !keep {
                    continue;
                }
                put_run_artifact(
                    &store,
                    &run_id,
                    &format!(
                        "artifact-{}-{index}-produced-{artifact_index}",
                        run_id.as_str()
                    ),
                    &artifact.kind,
                    &artifact.bytes,
                    &mut handles,
                )?;
            }
        }
        if let Some(protection) = live_protection_snapshot(&before, &graph_diff, &records)? {
            put_json_run_artifact(
                &store,
                &run_id,
                &format!("artifact-{}-protection", run_id.as_str()),
                "protection-snapshot",
                &protection,
                &mut handles,
            )?;
        }
        let summary = execution_summary(
            &run_id,
            &compiled.change,
            &before,
            &range,
            &cmd.scope,
            &effective_scope,
            &cmd.evidence_policy,
            outcome,
            &records,
        )?;
        put_run_artifact(
            &store,
            &run_id,
            &format!("artifact-{}-summary", run_id.as_str()),
            "execution-summary",
            &summary,
            &mut handles,
        )?;
        handles.sort();

        let state = RunState {
            id: run_id.to_string(),
            status: "complete".into(),
            outcome: outcome.into(),
            handles: handles.clone(),
        };
        *self.lock() = Some(state);
        Ok(RunReply {
            run_id: run_id.to_string(),
            change: compiled.change,
            base: range.base_ref,
            head: range.head_ref,
            requested_scope: cmd.scope.clone(),
            scope: effective_scope,
            status: "complete".into(),
            executed: true,
            outcome: outcome.into(),
            artifact_handles: handles,
        })
    }

    fn status(&self, cmd: &StatusCommand) -> Result<StatusReply, BusError> {
        if let Some(run) = self.lock().clone()
            && cmd.run_id.as_ref().is_none_or(|want| want == &run.id)
        {
            return Ok(StatusReply {
                run_id: Some(run.id.clone()),
                status: run.status.clone(),
                outcome: Some(run.outcome.clone()),
                handles: run.handles.clone(),
            });
        }

        let store = self.store()?;
        let stored = match &cmd.run_id {
            Some(want) => {
                let id = RunId::new(want).map_err(|err| BusError::Identity(err.to_string()))?;
                store
                    .get_run(&id)
                    .map_err(|err| BusError::Store(err.to_string()))?
            }
            None => store
                .latest_run_any()
                .map_err(|err| BusError::Store(err.to_string()))?,
        };
        match stored {
            Some(run) => {
                let handles = store
                    .run_artifacts(&run.id)
                    .map_err(|err| BusError::Store(err.to_string()))?
                    .into_iter()
                    .map(|id| id.to_string())
                    .collect();
                Ok(StatusReply {
                    run_id: Some(run.id.to_string()),
                    status: run.status,
                    outcome: Some(run.outcome),
                    handles,
                })
            }
            None if cmd.run_id.is_some() => Err(BusError::NotFound(format!(
                "run {}",
                cmd.run_id.as_deref().unwrap_or_default()
            ))),
            None => Ok(StatusReply {
                run_id: None,
                status: "idle".into(),
                outcome: None,
                handles: Vec::new(),
            }),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn verify(&self, cmd: &VerifyCommand) -> Result<VerifyReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let contract = load_quality_contract(&self.repo, &compiled.change)?;
        let oracle = seal(&contract, &compiled.obligations, &compiled.spec)?;
        let revision = self.revision()?;
        let store = self.store()?;
        let run = store
            .latest_run(&compiled.change, &revision)
            .map_err(|err| BusError::Store(err.to_string()))?;
        let artifact_ids = match &run {
            Some(run) => store
                .run_artifacts(&run.id)
                .map_err(|err| BusError::Store(err.to_string()))?,
            None => Vec::new(),
        };
        let mut present = Vec::new();
        let mut obligation_execution = BTreeMap::<String, Vec<String>>::new();
        for artifact in &artifact_ids {
            let (record, bytes) = store
                .read_artifact(artifact)
                .map_err(|err| BusError::Store(err.to_string()))?;
            if matches!(record.kind.as_str(), "coverage" | "lcov")
                && !present.contains(&EvidenceKind::Coverage)
            {
                present.push(EvidenceKind::Coverage);
            }
            if record.kind == "obligation-execution-map" {
                if !obligation_execution.is_empty() {
                    return Err(BusError::Store(
                        "run has more than one obligation execution map".into(),
                    ));
                }
                obligation_execution = parse_obligation_execution_map(&bytes)?;
            }
        }
        let mut proofs = Vec::new();
        let mut verdicts = Vec::new();
        for obligation in &compiled.obligations {
            let proof_suffix = run.as_ref().map_or_else(
                || sha256_hex(revision.as_str().as_bytes())[..16].to_owned(),
                |run| run.id.to_string(),
            );
            let id = ProofId::new(format!("proof-{}-{proof_suffix}", obligation.id))
                .map_err(|err| BusError::Identity(err.to_string()))?;
            let execution = if obligation_execution
                .get(obligation.id.as_str())
                .is_some_and(|tests| !tests.is_empty())
            {
                match &run {
                    Some(run) if run.passed => ExecutionEvidence::Passed {
                        present: present.clone(),
                    },
                    Some(_) => ExecutionEvidence::Failed {
                        seal_contradicted: false,
                        present: present.clone(),
                    },
                    None => ExecutionEvidence::Absent,
                }
            } else {
                ExecutionEvidence::Absent
            };
            let assembled = assemble(AssemblyInput {
                id: id.clone(),
                requirement: obligation.requirement.clone(),
                scenario: obligation.scenario.clone(),
                obligation: obligation.id.clone(),
                oracle_seal: oracle.id.clone(),
                revision: revision.clone(),
                program: None,
                run: run.as_ref().map(|item| item.id.clone()),
                observations: Vec::new(),
                artifacts: artifact_ids.clone(),
                required_evidence: obligation.required_evidence.clone(),
                execution,
                spec_ambiguous: false,
                quality_debt: Vec::new(),
                mutation: None,
            });
            if run.is_some()
                && store
                    .get_proof(&id)
                    .map_err(|err| BusError::Store(err.to_string()))?
                    .is_none()
            {
                store
                    .put_proof(&StoredProof {
                        id,
                        revision: revision.clone(),
                        obligation: obligation.id.clone(),
                        oracle_seal: oracle.id.clone(),
                        verdict: assembled.proof.verdict.as_str().into(),
                    })
                    .map_err(|err| BusError::Store(err.to_string()))?;
            }
            verdicts.push(assembled.proof.verdict);
            proofs.push(ProofSummary {
                id: assembled.proof.id.to_string(),
                requirement: obligation.requirement.to_string(),
                obligation: obligation.id.to_string(),
                verdict: assembled.proof.verdict.as_str().to_owned(),
            });
        }
        Ok(combine_verify(&compiled.change, proofs, &verdicts))
    }

    fn explain(&self, cmd: &ExplainCommand) -> Result<ExplainReply, BusError> {
        let store = self.store()?;
        if let Ok(id) = ProofId::new(&cmd.id)
            && let Some(proof) = store
                .get_proof(&id)
                .map_err(|err| BusError::Store(err.to_string()))?
        {
            return Ok(ExplainReply {
                id: cmd.id.clone(),
                kind: "proof".into(),
                summary: format!(
                    "proof {} is {} for obligation {}",
                    proof.id, proof.verdict, proof.obligation
                ),
                provenance: vec![
                    format!("revision {}", proof.revision),
                    format!("oracle seal {}", proof.oracle_seal),
                ],
            });
        }
        if let Ok(id) = RunId::new(&cmd.id)
            && let Some(run) = store
                .get_run(&id)
                .map_err(|err| BusError::Store(err.to_string()))?
        {
            let handles = store
                .run_artifacts(&run.id)
                .map_err(|err| BusError::Store(err.to_string()))?;
            return Ok(ExplainReply {
                id: cmd.id.clone(),
                kind: "run".into(),
                summary: format!(
                    "run {} completed with outcome {} for change {}",
                    run.id, run.outcome, run.change_id
                ),
                provenance: std::iter::once(format!("revision {}", run.revision))
                    .chain(
                        handles
                            .into_iter()
                            .map(|handle| format!("evidence {handle}")),
                    )
                    .collect(),
            });
        }
        for change in list_changes(&self.repo)? {
            let Ok(compiled) = self.compiled(&change) else {
                continue;
            };
            if let Some(obligation) = compiled
                .obligations
                .iter()
                .find(|item| item.id.as_str() == cmd.id)
            {
                return Ok(ExplainReply {
                    id: cmd.id.clone(),
                    kind: "obligation".into(),
                    summary: format!(
                        "obligation {} ({}) for {}",
                        obligation.id,
                        obligation_kind_token(obligation.kind),
                        obligation.requirement
                    ),
                    provenance: vec![format!("openspec/changes/{}/quality.yaml", compiled.change)],
                });
            }
        }
        if self.repo.join(".git").exists() {
            for change in list_changes(&self.repo)? {
                let selection = self.select(&working_tree_selection(change));
                let Ok(selection) = selection else {
                    continue;
                };
                if let Some(index) = selection.selected.iter().position(|item| item == &cmd.id) {
                    let mut provenance = selection
                        .explanations
                        .get(index)
                        .cloned()
                        .unwrap_or_default();
                    if let Some(revision) = selection.revision {
                        provenance.insert(0, format!("revision {revision}"));
                    }
                    return Ok(ExplainReply {
                        id: cmd.id.clone(),
                        kind: "selection".into(),
                        summary: format!("test {} selected by {}", cmd.id, selection.algorithm),
                        provenance,
                    });
                }
            }
            let revision = self.revision()?;
            let report = self.weavatrix_operation(
                &revision,
                "run_audit",
                &json!({"base_ref": "HEAD", "debt": "all", "max_findings": 5000}),
            )?;
            if let Some(reply) = explain_debt_finding(&report, &cmd.id, &revision) {
                return Ok(reply);
            }
        }
        Err(BusError::NotFound(format!("id {}", cmd.id)))
    }

    fn evidence(&self, cmd: &EvidenceCommand) -> Result<EvidenceReply, BusError> {
        let id = ArtifactId::new(&cmd.handle).map_err(|err| BusError::Identity(err.to_string()))?;
        let store = self.store()?;
        let (record, bytes) = match store.read_artifact(&id) {
            Ok(value) => value,
            Err(wvq_store::StoreError::MissingBlob(_)) => {
                return Err(BusError::NotFound(format!("handle {}", cmd.handle)));
            }
            Err(err) => return Err(BusError::Store(err.to_string())),
        };
        let inline_text = if bytes.len() <= INLINE_LIMIT {
            std::str::from_utf8(&bytes).ok().map(ToOwned::to_owned)
        } else {
            None
        };
        Ok(EvidenceReply {
            handle: cmd.handle.clone(),
            kind: record.kind,
            byte_len: record.byte_len,
            content_hash: Some(record.content_hash.to_string()),
            inline_text,
        })
    }

    fn spec_validate(&self, cmd: &SpecCommand) -> Result<SpecValidateReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        Ok(SpecValidateReply {
            change: compiled.change,
            requirements: unique_requirements(&compiled.obligations).len() as u64,
            obligations: compiled.obligations.len() as u64,
            ok: true,
        })
    }

    fn spec_seal(&self, cmd: &SpecCommand) -> Result<SpecSealReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let contract = load_quality_contract(&self.repo, &compiled.change)?;
        let oracle = seal(&contract, &compiled.obligations, &compiled.spec)?;
        Ok(SpecSealReply {
            change: compiled.change,
            seal_id: oracle.id.to_string(),
            digest: oracle.digest.to_string(),
            obligations: compiled.obligations.len() as u64,
        })
    }

    fn debt(&self, cmd: &DebtCommand) -> Result<DebtReply, BusError> {
        let _ = self.compiled(&cmd.change)?;
        let range = self.revision_range(&cmd.base, &cmd.head)?;
        let revision = self.revision()?;
        let report = self.weavatrix_operation(
            &revision,
            "run_audit",
            &json!({"base_ref": range.base_ref, "debt": "all", "max_findings": 5000}),
        )?;
        let debt = report
            .get("debt")
            .ok_or_else(|| BusError::Intelligence("run_audit omitted debt evidence".into()))?;
        let comparison_present = debt
            .pointer("/comparison/present")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if !comparison_present {
            return Err(BusError::Intelligence(
                debt.pointer("/comparison/reason")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("immutable debt comparison is unavailable")
                    .into(),
            ));
        }
        let counts = debt
            .get("counts")
            .ok_or_else(|| BusError::Intelligence("run_audit omitted debt counts".into()))?;
        let expected_new = count_field(counts, "new")?;
        let expected_existing = count_field(counts, "existing")?;
        let expected_fixed = count_field(counts, "fixed")?;
        let new_ids = debt_bucket_ids(debt, "new", expected_new)?;
        let existing_ids = debt_bucket_ids(debt, "existing", expected_existing)?;
        let fixed_ids = debt_bucket_ids(debt, "fixed", expected_fixed)?;
        let store = self.store()?;
        let previously_fixed = store
            .previously_fixed_debt()
            .map_err(|err| BusError::Store(err.to_string()))?
            .into_iter()
            .collect::<BTreeSet<_>>();
        let exceptions = load_debt_exceptions(&self.repo)?;
        let head_ids = new_ids
            .iter()
            .chain(existing_ids.iter())
            .cloned()
            .collect::<BTreeSet<_>>();
        let excepted = head_ids
            .intersection(&exceptions.active)
            .cloned()
            .collect::<BTreeSet<_>>();
        let returned = new_ids
            .intersection(&previously_fixed)
            .filter(|id| !excepted.contains(*id))
            .cloned()
            .collect::<BTreeSet<_>>();
        store
            .remember_fixed_debt(&fixed_ids.iter().cloned().collect::<Vec<_>>(), &revision)
            .map_err(|err| BusError::Store(err.to_string()))?;
        let mut limitations = debt
            .get("uncomparable_categories")
            .and_then(serde_json::Value::as_object)
            .into_iter()
            .flat_map(|items| items.iter())
            .map(|(kind, reason)| {
                format!("{kind}: {}", reason.as_str().unwrap_or("not comparable"))
            })
            .collect::<Vec<_>>();
        limitations.extend(exceptions.notes);
        Ok(DebtReply {
            base: range.base_ref,
            head: range.head_ref,
            revision: Some(revision.to_string()),
            comparison_present,
            existing: u64::try_from(existing_ids.difference(&excepted).count()).unwrap_or(u64::MAX),
            new: u64::try_from(
                new_ids
                    .difference(&excepted)
                    .filter(|id| !returned.contains(*id))
                    .count(),
            )
            .unwrap_or(u64::MAX),
            fixed: expected_fixed,
            returned: u64::try_from(returned.len()).unwrap_or(u64::MAX),
            excepted: u64::try_from(excepted.len()).unwrap_or(u64::MAX),
            findings: compact_debt_findings(debt, &returned, &excepted),
            limitations,
        })
    }

    fn select(&self, cmd: &SelectCommand) -> Result<SelectReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let range = self.revision_range(&cmd.base, &cmd.head)?;
        let revision = self.revision()?;
        let static_report = self.weavatrix_operation(
            &revision,
            "select_tests",
            &json!({
                "base_ref": range.base_ref,
                "max_tests": 500,
                "depth": 6,
                "max_nodes": 2000
            }),
        )?;
        let diff = self.weavatrix_operation(
            &revision,
            "graph_diff",
            &json!({
                "base_ref": range.base_ref,
                "detail": "edges",
                "max_results": 2000,
                "token_budget": 20000
            }),
        )?;
        let obligations: Vec<ObligationNeed> = compiled
            .obligations
            .iter()
            .map(|item| ObligationNeed {
                id: item.id.to_string(),
                high_risk: matches!(item.risk, RiskLevel::High | RiskLevel::Critical),
            })
            .collect();
        let selection = build_live_selection(&self.repo, &static_report, &diff, &obligations)?;
        let selection_complete = selection.complete();
        Ok(SelectReply {
            base: range.base_ref,
            head: range.head_ref,
            revision: Some(revision.to_string()),
            algorithm: "weavatrix-base-head-union+greedy-weighted-set-cover".into(),
            selected: selection.selected,
            uncovered_mandatory: selection.uncovered_mandatory,
            explanations: selection.explanations,
            executed: false,
            selection_complete,
        })
    }

    fn model(&self, cmd: &ModelCommand) -> Result<ModelReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let kind = parse_model_kind(&cmd.kind)?;
        let mut policy = load_model_policy(&self.repo)?;
        if let Some(hints) = load_quality_contract(&self.repo, &compiled.change)?.ai {
            policy.budget.planning_tokens =
                policy.budget.planning_tokens.min(hints.planning_tokens);
            policy.budget.runtime_tokens = policy.budget.runtime_tokens.min(hints.runtime_tokens);
        }
        let store = self.store()?;
        let persisted = store
            .ai_usage_for_change(&compiled.change)
            .map_err(|err| BusError::Store(err.to_string()))?
            .unwrap_or_default();
        let usage = AiUsage {
            planning_tokens: persisted.planning_tokens,
            runtime_tokens: persisted.runtime_tokens,
            browser_escape_calls: u32::try_from(persisted.browser_escape_calls).map_err(|_| {
                BusError::Store("persisted browser escape count exceeds u32".into())
            })?,
            vision_calls: u32::try_from(persisted.vision_calls)
                .map_err(|_| BusError::Store("persisted vision call count exceeds u32".into()))?,
            cost_micros: persisted.cost_micros,
        };
        let mut firewall = AiCostFirewall::with_usage(policy.budget, usage);
        let reply = call_local_model(
            &policy.model,
            &LocalModelRequest {
                kind,
                prompt: cmd.prompt.clone(),
            },
            &mut firewall,
        )
        .map_err(|err| BusError::Model(err.to_string()))?;
        let total_tokens = reply.input_tokens.saturating_add(reply.output_tokens);
        let (planning_tokens, runtime_tokens, browser_escape_calls, vision_calls) = match kind {
            AiCallKind::Planning => (total_tokens, 0, 0, 0),
            AiCallKind::Runtime => (0, total_tokens, 0, 0),
            AiCallKind::BrowserEscape => (0, total_tokens, 1, 0),
            AiCallKind::Vision => (0, total_tokens, 0, 1),
        };
        store
            .put_ai_usage(
                &make_ai_usage_id(&compiled.change, &cmd.kind)?,
                &StoredAiUsage {
                    change_id: compiled.change.clone(),
                    run_id: None,
                    planning_tokens,
                    runtime_tokens,
                    browser_escape_calls,
                    vision_calls,
                    cost_micros: reply.cost_micros,
                },
            )
            .map_err(|err| BusError::Store(err.to_string()))?;
        Ok(ModelReply {
            change: compiled.change,
            kind: cmd.kind.clone(),
            model: reply.model,
            text: reply.text,
            input_tokens: reply.input_tokens,
            output_tokens: reply.output_tokens,
            cost_micros: reply.cost_micros,
        })
    }

    fn changes(&self, cmd: &ChangesCommand) -> Result<ChangesReply, BusError> {
        let _ = cmd;
        Ok(ChangesReply {
            changes: list_changes(&self.repo)?,
        })
    }
}

fn obligation_kind_token(kind: ObligationKind) -> &'static str {
    match kind {
        ObligationKind::Behavioral => "behavioral",
        ObligationKind::Invariant => "invariant",
        ObligationKind::Api => "api",
        ObligationKind::Contract => "contract",
        ObligationKind::Permission => "permission",
        ObligationKind::Accessibility => "accessibility",
        ObligationKind::Visual => "visual",
        ObligationKind::Performance => "performance",
        ObligationKind::Architecture => "architecture",
        ObligationKind::CodeHealth => "code_health",
        ObligationKind::Coverage => "coverage",
        ObligationKind::Mutation => "mutation",
        ObligationKind::Metamorphic => "metamorphic",
        ObligationKind::SecurityPolicy => "security_policy",
    }
}

fn pack_context(change: &str, purpose: &str, budget: u64, items: Vec<String>) -> ContextReply {
    let (kept, used, truncated) = bound_items(items, budget.max(1));
    let mut requirements = Vec::new();
    let mut obligations = Vec::new();
    let mut heuristics = Vec::new();
    let mut coverage = Vec::new();
    for item in kept {
        if item.starts_with("obligation") {
            obligations.push(item);
        } else if item.starts_with("heuristic") {
            heuristics.push(item);
        } else if item.starts_with("coverage") {
            coverage.push(item);
        } else {
            requirements.push(item);
        }
    }
    ContextReply {
        change: change.to_owned(),
        purpose: purpose.to_owned(),
        requirements,
        obligations,
        heuristics,
        coverage,
        truncated,
        tokens_used: used,
        token_budget: budget.max(1),
    }
}

fn requirement_texts(spec: &OpenSpecChange) -> Vec<String> {
    let mut out = Vec::new();
    for capability in &spec.capabilities {
        for operation in &capability.operations {
            let delta = match operation {
                RequirementOp::Added(delta)
                | RequirementOp::Modified(delta)
                | RequirementOp::Removed(delta) => delta,
                RequirementOp::Renamed { from, to, location } => {
                    out.push(format!(
                        "requirement rename {from} → {to} ({}:{})",
                        location.file.display(),
                        location.line
                    ));
                    continue;
                }
            };
            out.push(format!(
                "requirement {} at {}:{}: {}",
                delta.id,
                delta.location.file.display(),
                delta.location.line,
                delta.name
            ));
        }
    }
    out
}

fn obligation_texts(obligations: &[TestObligation]) -> Vec<String> {
    obligations
        .iter()
        .map(|item| {
            format!(
                "obligation {} {} risk {}",
                item.id,
                obligation_kind_token(item.kind),
                risk_token(item.risk)
            )
        })
        .collect()
}

fn unique_requirements(obligations: &[TestObligation]) -> Vec<String> {
    let mut out = Vec::new();
    for item in obligations {
        let id = item.requirement.to_string();
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

fn risk_token(level: RiskLevel) -> &'static str {
    match level {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
        RiskLevel::Critical => "critical",
    }
}

fn deterministic_checks() -> Vec<String> {
    vec![
        "architecture".into(),
        "size".into(),
        "dead_code".into(),
        "clones".into(),
        "topology".into(),
        "api".into(),
        "history".into(),
        "coverage".into(),
    ]
}

fn empty_debt(base: &str, head: &str) -> DebtReply {
    DebtReply {
        base: base.to_owned(),
        head: head.to_owned(),
        revision: None,
        comparison_present: false,
        existing: 0,
        new: 0,
        fixed: 0,
        returned: 0,
        excepted: 0,
        findings: Vec::new(),
        limitations: Vec::new(),
    }
}

fn working_tree_selection(change: String) -> SelectCommand {
    SelectCommand {
        change,
        base: "HEAD".into(),
        head: "WORKTREE".into(),
    }
}

fn validate_purpose(purpose: &str) -> Result<(), BusError> {
    match purpose {
        "spec" | "implementation" | "review" => Ok(()),
        other => Err(BusError::Unknown {
            field: "purpose",
            value: other.to_owned(),
        }),
    }
}

fn validate_scope(scope: &str) -> Result<(), BusError> {
    match scope {
        "impacted" | "all" => Ok(()),
        other => Err(BusError::Unknown {
            field: "scope",
            value: other.to_owned(),
        }),
    }
}

fn validate_evidence_policy(policy: &str) -> Result<(), BusError> {
    match policy {
        "standard" | "minimal" | "none" => Ok(()),
        other => Err(BusError::Unknown {
            field: "evidence_policy",
            value: other.to_owned(),
        }),
    }
}

fn validate_revision_ref(field: &'static str, reference: &str) -> Result<(), BusError> {
    if reference.is_empty()
        || reference.len() > 512
        || reference.starts_with('-')
        || reference
            .bytes()
            .any(|byte| matches!(byte, 0 | b'\n' | b'\r'))
    {
        return Err(BusError::Unknown {
            field,
            value: reference.to_owned(),
        });
    }
    if field == "base" && reference == "WORKTREE" {
        return Err(BusError::Unknown {
            field,
            value: reference.to_owned(),
        });
    }
    Ok(())
}

fn parse_model_kind(kind: &str) -> Result<AiCallKind, BusError> {
    match kind {
        "planning" => Ok(AiCallKind::Planning),
        "runtime" => Ok(AiCallKind::Runtime),
        "browser_escape" => Ok(AiCallKind::BrowserEscape),
        "vision" => Ok(AiCallKind::Vision),
        other => Err(BusError::Unknown {
            field: "model kind",
            value: other.to_owned(),
        }),
    }
}

fn list_changes(repo: &Path) -> Result<Vec<String>, BusError> {
    let dir = repo.join("openspec").join("changes");
    let entries = std::fs::read_dir(&dir)
        .map_err(|err| BusError::NotFound(format!("openspec/changes: {err}")))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| BusError::NotFound(err.to_string()))?;
        if entry.file_type().map(|ty| ty.is_dir()).unwrap_or(false) {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

fn resolve_change(repo: &Path, change: &str) -> Result<String, BusError> {
    if change != "current" {
        return Ok(change.to_owned());
    }
    let names = list_changes(repo)?;
    match names.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(BusError::NotFound("no OpenSpec changes".into())),
        _ => Err(BusError::Ambiguous(
            "change=current is ambiguous; pass a change id".into(),
        )),
    }
}

fn verify_from_token(change: &str, verdict: &str) -> VerifyReply {
    let blocking = verdict == "CONTRADICTED";
    VerifyReply {
        change: change.to_owned(),
        verdict: verdict.to_owned(),
        blocking,
        proofs: vec![ProofSummary {
            id: "proof-fake".into(),
            requirement: "sankey.visual-limit-others".into(),
            obligation: "others-visible".into(),
            verdict: verdict.to_owned(),
        }],
    }
}

fn combine_verify(
    change: &str,
    proofs: Vec<ProofSummary>,
    verdicts: &[ProofVerdict],
) -> VerifyReply {
    let combined = combine_verdicts(verdicts);
    VerifyReply {
        change: change.to_owned(),
        verdict: combined.as_str().to_owned(),
        blocking: combined == ProofVerdict::Contradicted,
        proofs,
    }
}

fn combine_verdicts(verdicts: &[ProofVerdict]) -> ProofVerdict {
    if verdicts.is_empty() {
        return ProofVerdict::Unproven;
    }
    if verdicts.contains(&ProofVerdict::Contradicted) {
        return ProofVerdict::Contradicted;
    }
    if verdicts.contains(&ProofVerdict::HumanRequired) {
        return ProofVerdict::HumanRequired;
    }
    if verdicts.iter().all(|item| *item == ProofVerdict::Proven) {
        return ProofVerdict::Proven;
    }
    if verdicts.iter().all(|item| *item == ProofVerdict::Unproven) {
        return ProofVerdict::Unproven;
    }
    ProofVerdict::Partial
}

fn count_field(counts: &Value, name: &str) -> Result<u64, BusError> {
    counts
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| BusError::Intelligence(format!("run_audit omitted debt count {name}")))
}

fn debt_bucket_ids(
    debt: &Value,
    bucket: &str,
    expected: u64,
) -> Result<BTreeSet<String>, BusError> {
    let items = debt
        .pointer(&format!("/findings/{bucket}"))
        .and_then(Value::as_array)
        .ok_or_else(|| BusError::Intelligence(format!("run_audit omitted debt bucket {bucket}")))?;
    if u64::try_from(items.len()).unwrap_or(u64::MAX) != expected {
        return Err(BusError::Intelligence(format!(
            "run_audit debt bucket {bucket} is incomplete: expected {expected}, received {}",
            items.len()
        )));
    }
    items
        .iter()
        .map(|item| {
            item.as_str()
                .or_else(|| item.get("id").and_then(Value::as_str))
                .filter(|id| !id.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| {
                    BusError::Intelligence(format!(
                        "run_audit debt bucket {bucket} contains a finding without id"
                    ))
                })
        })
        .collect()
}

#[derive(Default)]
struct DebtExceptions {
    active: BTreeSet<String>,
    notes: Vec<String>,
}

fn load_debt_exceptions(repo: &Path) -> Result<DebtExceptions, BusError> {
    let path = repo.join(".weavatrix-quality").join("config.yaml");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DebtExceptions::default());
        }
        Err(err) => {
            return Err(BusError::Runtime(format!(
                "cannot read quality policy {}: {err}",
                path.display()
            )));
        }
    };
    let value: serde_yaml::Value = serde_yaml::from_str(&raw).map_err(|err| {
        BusError::Runtime(format!("invalid quality policy {}: {err}", path.display()))
    })?;
    let root = value.as_mapping().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} must be a mapping",
            path.display()
        ))
    })?;
    let version = yaml_get(root, "quality_policy_v")
        .and_then(serde_yaml::Value::as_u64)
        .ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} is missing quality_policy_v",
                path.display()
            ))
        })?;
    if version != 1 {
        return Err(BusError::Runtime(format!(
            "unknown quality_policy_v {version} in {}",
            path.display()
        )));
    }
    let Some(ratchet) = yaml_get(root, "ratchet") else {
        return Ok(DebtExceptions::default());
    };
    let ratchet = ratchet.as_mapping().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} ratchet must be a mapping",
            path.display()
        ))
    })?;
    let Some(exceptions) = yaml_get(ratchet, "exceptions") else {
        return Ok(DebtExceptions::default());
    };
    let exceptions = exceptions.as_sequence().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} ratchet.exceptions must be a list",
            path.display()
        ))
    })?;
    let today = utc_date();
    let mut out = DebtExceptions::default();
    for (index, item) in exceptions.iter().enumerate() {
        let item = item.as_mapping().ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} exception {} must be a mapping",
                path.display(),
                index + 1
            ))
        })?;
        let fingerprint = yaml_string(item, "fingerprint", &path, index)?;
        let _reason = yaml_string(item, "reason", &path, index)?;
        if let Some(expires) = yaml_get(item, "expires") {
            let expires = expires
                .as_str()
                .filter(|date| valid_iso_date(date))
                .ok_or_else(|| {
                    BusError::Runtime(format!(
                        "quality policy {} exception {} has invalid expires date",
                        path.display(),
                        index + 1
                    ))
                })?;
            if expires < today.as_str() {
                out.notes.push(format!(
                    "expired debt exception {fingerprint} (expired {expires})"
                ));
                continue;
            }
        }
        out.active.insert(fingerprint);
    }
    Ok(out)
}

#[allow(clippy::too_many_lines)]
fn load_test_bindings(repo: &Path) -> Result<Vec<TestBinding>, BusError> {
    let path = repo.join(".weavatrix-quality").join("config.yaml");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(BusError::Runtime(format!(
                "cannot read quality policy {}: {err}",
                path.display()
            )));
        }
    };
    let value: serde_yaml::Value = serde_yaml::from_str(&raw).map_err(|err| {
        BusError::Runtime(format!("invalid quality policy {}: {err}", path.display()))
    })?;
    let root = value.as_mapping().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} must be a mapping",
            path.display()
        ))
    })?;
    let version = yaml_get(root, "quality_policy_v")
        .and_then(serde_yaml::Value::as_u64)
        .ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} is missing quality_policy_v",
                path.display()
            ))
        })?;
    if version != 1 {
        return Err(BusError::Runtime(format!(
            "unknown quality_policy_v {version} in {}",
            path.display()
        )));
    }
    let Some(bindings) = yaml_get(root, "test_bindings") else {
        return Ok(Vec::new());
    };
    let bindings = bindings.as_sequence().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} test_bindings must be a list",
            path.display()
        ))
    })?;
    let mut out = Vec::new();
    for (index, binding) in bindings.iter().enumerate() {
        let binding = binding.as_mapping().ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} test binding {} must be a mapping",
                path.display(),
                index + 1
            ))
        })?;
        let test_path = normalize_path(&yaml_string(binding, "path", &path, index)?);
        let parsed_path = Path::new(&test_path);
        if parsed_path.is_absolute()
            || parsed_path.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(BusError::Runtime(format!(
                "quality policy {} test binding {} path must stay repository-relative",
                path.display(),
                index + 1
            )));
        }
        let obligations = yaml_get(binding, "obligations")
            .and_then(serde_yaml::Value::as_sequence)
            .ok_or_else(|| {
                BusError::Runtime(format!(
                    "quality policy {} test binding {} requires obligations",
                    path.display(),
                    index + 1
                ))
            })?
            .iter()
            .map(|obligation| {
                obligation
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| {
                        BusError::Runtime(format!(
                            "quality policy {} test binding {} has invalid obligation",
                            path.display(),
                            index + 1
                        ))
                    })
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        if obligations.is_empty() {
            return Err(BusError::Runtime(format!(
                "quality policy {} test binding {} has no obligations",
                path.display(),
                index + 1
            )));
        }
        let cost = yaml_get(binding, "cost").map_or(Ok(100), |value| {
            value.as_u64().filter(|cost| *cost > 0).ok_or_else(|| {
                BusError::Runtime(format!(
                    "quality policy {} test binding {} cost must be positive",
                    path.display(),
                    index + 1
                ))
            })
        })?;
        let flake_penalty = yaml_get(binding, "flake_penalty").map_or(Ok(0), |value| {
            value.as_u64().ok_or_else(|| {
                BusError::Runtime(format!(
                    "quality policy {} test binding {} flake_penalty must be an integer",
                    path.display(),
                    index + 1
                ))
            })
        })?;
        out.push(TestBinding {
            path: test_path,
            obligations,
            cost,
            flake_penalty,
        });
    }
    Ok(out)
}

fn load_model_policy(repo: &Path) -> Result<ModelPolicy, BusError> {
    let path = repo.join(".weavatrix-quality").join("config.yaml");
    let raw = std::fs::read_to_string(&path).map_err(|err| {
        BusError::Model(format!(
            "cannot read model policy {}: {err}",
            path.display()
        ))
    })?;
    let value: serde_yaml::Value = serde_yaml::from_str(&raw).map_err(|err| {
        BusError::Model(format!("invalid model policy {}: {err}", path.display()))
    })?;
    let root = value.as_mapping().ok_or_else(|| {
        BusError::Model(format!("model policy {} must be a mapping", path.display()))
    })?;
    let version = yaml_get(root, "quality_policy_v")
        .and_then(serde_yaml::Value::as_u64)
        .ok_or_else(|| {
            BusError::Model(format!(
                "model policy {} is missing quality_policy_v",
                path.display()
            ))
        })?;
    if version != 1 {
        return Err(BusError::Model(format!(
            "unknown quality_policy_v {version} in {}",
            path.display()
        )));
    }
    let ai = yaml_get(root, "ai")
        .and_then(serde_yaml::Value::as_mapping)
        .ok_or_else(|| BusError::Model(format!("model policy {} requires ai", path.display())))?;
    let endpoint = yaml_required_string(ai, "endpoint", &path)?;
    let model = yaml_required_string(ai, "model", &path)?;
    let max_output_tokens = yaml_required_positive_u64(ai, "max_output_tokens", &path)?;
    let planning_tokens = yaml_required_u64(ai, "max_tokens_per_change", &path)?;
    let runtime_tokens = yaml_required_u64(ai, "max_runtime_tokens", &path)?;
    let browser_escape_calls =
        u32::try_from(yaml_required_u64(ai, "max_browser_escape_calls", &path)?).map_err(|_| {
            BusError::Model(format!(
                "model policy {} max_browser_escape_calls exceeds u32",
                path.display()
            ))
        })?;
    let vision_calls =
        u32::try_from(yaml_required_u64(ai, "max_vision_calls", &path)?).map_err(|_| {
            BusError::Model(format!(
                "model policy {} max_vision_calls exceeds u32",
                path.display()
            ))
        })?;
    let max_cost_micros = yaml_get(ai, "max_cost_micros")
        .map(|value| {
            value.as_u64().ok_or_else(|| {
                BusError::Model(format!(
                    "model policy {} max_cost_micros must be an integer",
                    path.display()
                ))
            })
        })
        .transpose()?;
    let input_micros_per_million = yaml_optional_u64(ai, "input_micros_per_million", &path)?;
    let output_micros_per_million = yaml_optional_u64(ai, "output_micros_per_million", &path)?;
    Ok(ModelPolicy {
        model: LocalModelConfig {
            endpoint,
            model,
            max_output_tokens,
            input_micros_per_million,
            output_micros_per_million,
        },
        budget: AiBudget {
            planning_tokens,
            runtime_tokens,
            browser_escape_calls,
            vision_calls,
            max_cost_micros,
        },
    })
}

fn yaml_get<'a>(mapping: &'a serde_yaml::Mapping, key: &str) -> Option<&'a serde_yaml::Value> {
    mapping.get(serde_yaml::Value::String(key.to_owned()))
}

fn yaml_required_string(
    mapping: &serde_yaml::Mapping,
    key: &str,
    path: &Path,
) -> Result<String, BusError> {
    yaml_get(mapping, key)
        .and_then(serde_yaml::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            BusError::Model(format!(
                "model policy {} requires non-empty {key}",
                path.display()
            ))
        })
}

fn yaml_required_u64(
    mapping: &serde_yaml::Mapping,
    key: &str,
    path: &Path,
) -> Result<u64, BusError> {
    yaml_get(mapping, key)
        .and_then(serde_yaml::Value::as_u64)
        .ok_or_else(|| {
            BusError::Model(format!(
                "model policy {} requires integer {key}",
                path.display()
            ))
        })
}

fn yaml_required_positive_u64(
    mapping: &serde_yaml::Mapping,
    key: &str,
    path: &Path,
) -> Result<u64, BusError> {
    yaml_required_u64(mapping, key, path).and_then(|value| {
        if value == 0 {
            Err(BusError::Model(format!(
                "model policy {} requires positive {key}",
                path.display()
            )))
        } else {
            Ok(value)
        }
    })
}

fn yaml_optional_u64(
    mapping: &serde_yaml::Mapping,
    key: &str,
    path: &Path,
) -> Result<u64, BusError> {
    yaml_get(mapping, key).map_or(Ok(0), |value| {
        value.as_u64().ok_or_else(|| {
            BusError::Model(format!(
                "model policy {} {key} must be an integer",
                path.display()
            ))
        })
    })
}

fn yaml_string(
    mapping: &serde_yaml::Mapping,
    key: &str,
    path: &Path,
    index: usize,
) -> Result<String, BusError> {
    yaml_get(mapping, key)
        .and_then(serde_yaml::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} exception {} requires non-empty {key}",
                path.display(),
                index + 1
            ))
        })
}

fn valid_iso_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
        && date[5..7]
            .parse::<u8>()
            .is_ok_and(|month| (1..=12).contains(&month))
        && date[8..10]
            .parse::<u8>()
            .is_ok_and(|day| (1..=31).contains(&day))
}

fn utc_date() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() / 86_400);
    let z = i64::try_from(days)
        .unwrap_or(i64::MAX)
        .saturating_add(719_468);
    let era = z / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn compact_debt_findings(
    debt: &Value,
    returned: &BTreeSet<String>,
    excepted: &BTreeSet<String>,
) -> Vec<String> {
    let Some(findings) = debt.get("findings") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (bucket, items) in ["new", "existing", "fixed"]
        .into_iter()
        .filter_map(|bucket| {
            findings
                .get(bucket)
                .and_then(Value::as_array)
                .map(|items| (bucket, items))
        })
    {
        for item in items {
            let id = item
                .as_str()
                .or_else(|| item.get("id").and_then(Value::as_str))
                .unwrap_or("unknown-finding");
            if (bucket == "new" && returned.contains(id)) || excepted.contains(id) {
                continue;
            }
            let rule = item.get("rule").and_then(Value::as_str).unwrap_or("");
            out.push(if rule.is_empty() {
                format!("{bucket}: {id}")
            } else {
                format!("{bucket}: {id} ({rule})")
            });
        }
    }
    out.extend(returned.iter().map(|id| format!("returned: {id}")));
    out.extend(excepted.iter().map(|id| format!("excepted: {id}")));
    out
}

fn explain_debt_finding(report: &Value, id: &str, revision: &RevisionId) -> Option<ExplainReply> {
    let findings = report.pointer("/debt/findings")?;
    for bucket in ["new", "existing", "fixed"] {
        for finding in findings
            .get(bucket)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let found = finding
                .as_str()
                .or_else(|| finding.get("id").and_then(Value::as_str));
            if found != Some(id) {
                continue;
            }
            let mut provenance = vec![
                format!("revision {revision}"),
                format!("debt bucket {bucket}"),
            ];
            if let Some(rule) = finding.get("rule").and_then(Value::as_str) {
                provenance.push(format!("rule {rule}"));
            }
            if let Some(path) = finding
                .get("path")
                .or_else(|| finding.get("file"))
                .and_then(Value::as_str)
            {
                let line = finding.get("line").and_then(Value::as_u64);
                provenance.push(line.map_or_else(
                    || format!("source {path}"),
                    |line| format!("source {path}:{line}"),
                ));
            }
            let detail = finding
                .get("message")
                .and_then(Value::as_str)
                .map_or_else(String::new, |message| format!(": {message}"));
            return Some(ExplainReply {
                id: id.to_owned(),
                kind: "finding".into(),
                summary: format!("{bucket} debt finding {id}{detail}"),
                provenance,
            });
        }
    }
    None
}

fn static_and_base_tests(static_report: &Value, diff: &Value) -> (Vec<String>, Vec<Vec<String>>) {
    let mut reasons = BTreeMap::<String, BTreeSet<String>>::new();
    for test in static_report
        .get("tests")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(path) = test.get("path").and_then(Value::as_str) else {
            continue;
        };
        let entry = reasons.entry(normalize_path(path)).or_default();
        entry.insert("selected by Weavatrix head static impact".into());
        for reason in test
            .get("reasons")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            entry.insert(format!("head evidence: {reason}"));
        }
    }

    for node in diff
        .pointer("/nodes/removed")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(path) = node
            .get("label")
            .and_then(Value::as_str)
            .filter(|path| is_test_path(path))
        {
            reasons
                .entry(normalize_path(path))
                .or_default()
                .insert("base-only test preserved from graph_diff removed nodes".into());
        }
    }
    for edge in diff
        .pointer("/edges/removed")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for key in ["source", "target"] {
            if let Some(path) = edge
                .get(key)
                .and_then(Value::as_str)
                .and_then(test_path_from_node_id)
            {
                reasons
                    .entry(path)
                    .or_default()
                    .insert("base-only test preserved from graph_diff removed edge".into());
            }
        }
    }

    let selected = reasons.keys().cloned().collect::<Vec<_>>();
    let explanations = reasons
        .into_values()
        .map(|items| items.into_iter().collect())
        .collect();
    (selected, explanations)
}

fn build_live_selection(
    repo: &Path,
    static_report: &Value,
    diff: &Value,
    obligations: &[ObligationNeed],
) -> Result<LiveSelection, BusError> {
    let (static_selected, static_explanations) = static_and_base_tests(static_report, diff);
    let mut explanations = static_selected
        .iter()
        .cloned()
        .zip(static_explanations)
        .map(|(path, reasons)| (path, reasons.into_iter().collect::<BTreeSet<_>>()))
        .collect::<BTreeMap<_, _>>();
    let known = obligations
        .iter()
        .map(|obligation| obligation.id.clone())
        .collect::<BTreeSet<_>>();
    let mut merged = BTreeMap::<String, TestBinding>::new();
    for binding in load_test_bindings(repo)? {
        if let Some(unknown) = binding
            .obligations
            .iter()
            .find(|obligation| !known.contains(*obligation))
        {
            return Err(BusError::Runtime(format!(
                "test binding {} names unknown obligation {unknown}",
                binding.path
            )));
        }
        let entry = merged
            .entry(binding.path.clone())
            .or_insert_with(|| TestBinding {
                path: binding.path.clone(),
                obligations: BTreeSet::new(),
                cost: binding.cost,
                flake_penalty: binding.flake_penalty,
            });
        entry.obligations.extend(binding.obligations);
        entry.cost = entry.cost.min(binding.cost);
        entry.flake_penalty = entry.flake_penalty.max(binding.flake_penalty);
    }
    let bindings = merged.into_values().collect::<Vec<_>>();
    let candidates = bindings
        .iter()
        .filter(|binding| repo.join(&binding.path).is_file())
        .map(|binding| TestCandidate {
            id: binding.path.clone(),
            cost: binding.cost,
            flake_penalty: binding.flake_penalty,
            covers: binding.obligations.clone(),
            explanation: vec![format!(
                "quality policy binds this test to: {}",
                binding
                    .obligations
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )],
        })
        .collect();
    let plan = select_minimal_plan(SelectionInput {
        candidates,
        obligations: obligations.to_owned(),
    });
    let mut selected = static_selected.into_iter().collect::<BTreeSet<_>>();
    for test in plan.selected {
        selected.insert(test.id.clone());
        explanations
            .entry(test.id)
            .or_default()
            .extend(test.explanation);
    }
    let covered = bindings
        .iter()
        .filter(|binding| selected.contains(&binding.path) && repo.join(&binding.path).is_file())
        .flat_map(|binding| binding.obligations.iter().cloned())
        .collect::<BTreeSet<_>>();
    let uncovered_all = known.difference(&covered).cloned().collect::<Vec<_>>();
    let selected = selected.into_iter().collect::<Vec<_>>();
    let explanations = selected
        .iter()
        .map(|path| {
            explanations
                .remove(path)
                .unwrap_or_else(|| BTreeSet::from(["selected by live policy".into()]))
                .into_iter()
                .collect()
        })
        .collect();
    Ok(LiveSelection {
        selected,
        explanations,
        uncovered_mandatory: plan.uncovered_mandatory,
        uncovered_all,
        bindings,
    })
}

fn build_execution_requests(
    repo: &Path,
    targets: &[ExecutorTarget],
    selection: &LiveSelection,
    requested_scope: &str,
) -> (Vec<ExecutionRequest>, String, Option<BTreeSet<String>>) {
    if requested_scope != "impacted" || !selection.complete() || selection.selected.is_empty() {
        return full_execution_requests(targets);
    }
    let mut requests = BTreeMap::<(String, String, String), ExecutionRequest>::new();
    let mut executed = BTreeSet::new();
    for selected in &selection.selected {
        let absolute = repo.join(selected);
        if !absolute.is_file() {
            return full_execution_requests(targets);
        }
        let mut matching = targets
            .iter()
            .filter(|target| absolute.starts_with(&target.cwd))
            .collect::<Vec<_>>();
        matching.sort_by_key(|target| std::cmp::Reverse(target.cwd.components().count()));
        let Some(target) = matching.into_iter().find(|target| {
            matches!(
                target.executor.as_str(),
                "npm-test" | "vitest" | "jest" | "bun-test" | "playwright"
            )
        }) else {
            return full_execution_requests(targets);
        };
        let filter = absolute
            .strip_prefix(&target.cwd)
            .ok()
            .map(|path| normalize_path(&path.to_string_lossy()))
            .filter(|path| !path.is_empty());
        let Some(filter) = filter else {
            return full_execution_requests(targets);
        };
        let cwd = target.cwd.display().to_string();
        requests.insert(
            (target.executor.as_str().to_owned(), cwd, filter.clone()),
            ExecutionRequest {
                target: target.clone(),
                filter: Some(filter),
                selected_test: Some(selected.clone()),
            },
        );
        executed.insert(selected.clone());
    }
    if requests.is_empty() {
        full_execution_requests(targets)
    } else {
        (
            requests.into_values().collect(),
            "impacted".into(),
            Some(executed),
        )
    }
}

fn full_execution_requests(
    targets: &[ExecutorTarget],
) -> (Vec<ExecutionRequest>, String, Option<BTreeSet<String>>) {
    (
        targets
            .iter()
            .cloned()
            .map(|target| ExecutionRequest {
                target,
                filter: None,
                selected_test: None,
            })
            .collect(),
        "all".into(),
        None,
    )
}

fn test_path_from_node_id(id: &str) -> Option<String> {
    let raw = id.strip_prefix("file:").unwrap_or(id);
    let path = raw.split('#').next().unwrap_or(raw);
    is_test_path(path).then(|| normalize_path(path))
}

fn is_test_path(path: &str) -> bool {
    let path = normalize_path(path).to_ascii_lowercase();
    let file = path.rsplit('/').next().unwrap_or(&path);
    path.contains("/tests/")
        || path.starts_with("tests/")
        || path.contains("/__tests__/")
        || file.ends_with("_test.go")
        || file.starts_with("test_")
        || file.contains(".test.")
        || file.contains(".spec.")
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn make_run_id(change: &str, revision: &RevisionId) -> Result<RunId, BusError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| BusError::Identity(err.to_string()))?
        .as_nanos();
    let seed = format!(
        "{change}\0{}\0{nanos}\0{}",
        revision.as_str(),
        std::process::id()
    );
    let digest = sha256_hex(seed.as_bytes());
    RunId::new(format!("run-{}-{nanos}", &digest[..16]))
        .map_err(|err| BusError::Identity(err.to_string()))
}

fn make_ai_usage_id(change: &str, kind: &str) -> Result<String, BusError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| BusError::Identity(err.to_string()))?
        .as_nanos();
    let seed = format!("{change}\0{kind}\0{nanos}\0{}", std::process::id());
    Ok(format!("ai-{}-{nanos}", &sha256_hex(seed.as_bytes())[..16]))
}

fn put_run_artifact(
    store: &Store,
    run: &RunId,
    raw_id: &str,
    kind: &str,
    bytes: &[u8],
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let id = ArtifactId::new(raw_id).map_err(|err| BusError::Identity(err.to_string()))?;
    store
        .put_artifact(&id, kind, bytes)
        .map_err(|err| BusError::Store(err.to_string()))?;
    store
        .attach_run_artifact(run, &id)
        .map_err(|err| BusError::Store(err.to_string()))?;
    handles.push(id.to_string());
    Ok(())
}

fn put_json_run_artifact<T: serde::Serialize>(
    store: &Store,
    run: &RunId,
    raw_id: &str,
    kind: &str,
    value: &T,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| BusError::Runtime(format!("cannot encode {kind}: {err}")))?;
    put_run_artifact(store, run, raw_id, kind, &bytes, handles)
}

fn obligation_execution_map(
    repo: &Path,
    targets: &[ExecutorTarget],
    bindings: &[TestBinding],
    executed_tests: Option<&BTreeSet<String>>,
) -> Value {
    let mut obligations = BTreeMap::<String, BTreeSet<String>>::new();
    for binding in bindings {
        if executed_tests.is_some_and(|tests| !tests.contains(&binding.path)) {
            continue;
        }
        let path = repo.join(&binding.path);
        if !path.is_file() || !targets.iter().any(|target| path.starts_with(&target.cwd)) {
            continue;
        }
        for obligation in &binding.obligations {
            obligations
                .entry(obligation.clone())
                .or_default()
                .insert(binding.path.clone());
        }
    }
    json!({
        "schema_v": 1,
        "obligations": obligations.into_iter().map(|(id, tests)| {
            (id, tests.into_iter().collect::<Vec<_>>())
        }).collect::<BTreeMap<_, _>>()
    })
}

fn parse_obligation_execution_map(bytes: &[u8]) -> Result<BTreeMap<String, Vec<String>>, BusError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|err| BusError::Store(format!("invalid obligation execution map: {err}")))?;
    if value.get("schema_v").and_then(Value::as_u64) != Some(1) {
        return Err(BusError::Store(
            "unknown obligation execution map schema".into(),
        ));
    }
    let entries = value
        .get("obligations")
        .and_then(Value::as_object)
        .ok_or_else(|| BusError::Store("obligation execution map omitted obligations".into()))?;
    entries
        .iter()
        .map(|(obligation, tests)| {
            let tests = tests
                .as_array()
                .ok_or_else(|| {
                    BusError::Store(format!(
                        "obligation execution map {obligation} must be an array"
                    ))
                })?
                .iter()
                .map(|test| {
                    test.as_str()
                        .filter(|test| !test.is_empty())
                        .map(ToOwned::to_owned)
                        .ok_or_else(|| {
                            BusError::Store(format!(
                                "obligation execution map {obligation} has invalid test identity"
                            ))
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((obligation.clone(), tests))
        })
        .collect()
}

fn live_impacted_surface(
    diff: &Value,
    impact: &Value,
) -> Result<wvq_intelligence::ImpactedSurface, BusError> {
    ensure_complete_diff(diff)?;
    let mut base = Vec::new();
    let mut head = Vec::new();
    let mut removed_nodes = Vec::new();
    let mut removed_edges = Vec::new();

    for node in values_at(diff, "/nodes/removed") {
        if let Some(id) = graph_node_id(node) {
            base.push(id.clone());
            removed_nodes.push(id);
        }
    }
    for node in values_at(diff, "/nodes/added") {
        if let Some(id) = graph_node_id(node) {
            head.push(id);
        }
    }
    for changed in values_at(diff, "/nodes/changed") {
        if let Some(id) = changed.get("before").and_then(graph_node_id) {
            base.push(id);
        }
        if let Some(id) = changed.get("after").and_then(graph_node_id) {
            head.push(id);
        }
    }
    for edge in values_at(diff, "/edges/removed") {
        let source = edge
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let target = edge
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !source.is_empty() {
            base.push(source.to_owned());
        }
        if !target.is_empty() {
            base.push(target.to_owned());
        }
        removed_edges.push(format!("{source}->{target}"));
    }
    for edge in values_at(diff, "/edges/added") {
        if let Some(source) = edge.get("source").and_then(Value::as_str) {
            head.push(source.to_owned());
        }
        if let Some(target) = edge.get("target").and_then(Value::as_str) {
            head.push(target.to_owned());
        }
    }
    for node in values_at(impact, "/impacted_nodes") {
        if let Some(id) = graph_node_id(node) {
            head.push(id);
        }
    }

    let surfaces = SurfaceDelta {
        added: surface_labels(values_at(diff, "/nodes/added")),
        removed: surface_labels(values_at(diff, "/nodes/removed")),
    };
    Ok(impacted_surface(
        &base,
        &head,
        &GraphDelta {
            removed_nodes,
            removed_edges,
        },
        &surfaces,
    ))
}

fn ensure_complete_diff(diff: &Value) -> Result<(), BusError> {
    for (count, values) in [
        ("nodes_added", "/nodes/added"),
        ("nodes_removed", "/nodes/removed"),
        ("nodes_changed", "/nodes/changed"),
        ("edges_added", "/edges/added"),
        ("edges_removed", "/edges/removed"),
    ] {
        let expected = diff
            .pointer(&format!("/counts/{count}"))
            .and_then(Value::as_u64)
            .ok_or_else(|| BusError::Intelligence(format!("graph_diff omitted {count}")))?;
        let present = u64::try_from(values_at(diff, values).len()).unwrap_or(u64::MAX);
        if expected != present {
            return Err(BusError::Intelligence(format!(
                "graph_diff {count} is incomplete: expected {expected}, received {present}"
            )));
        }
    }
    Ok(())
}

fn live_protection_snapshot(
    revision: &RevisionId,
    diff: &Value,
    records: &[ExecutorRecord],
) -> Result<Option<wvq_proof::ProtectionSnapshot>, BusError> {
    let graph = head_coverage_graph(diff);
    let Some(nodes) = graph.get("nodes").and_then(Value::as_array) else {
        return Ok(None);
    };
    if nodes.is_empty() {
        return Ok(None);
    }
    let mut flows = BTreeMap::<String, FlowProtection>::new();
    for node in nodes {
        let Some(id) = graph_node_id(node) else {
            return Err(BusError::Intelligence(
                "impacted head graph contains a node without identity".into(),
            ));
        };
        flows.entry(id.clone()).or_insert_with(|| FlowProtection {
            flow: id,
            revision: revision.to_string(),
            tests: Vec::new(),
            sessions: Vec::new(),
            covered_nodes: Vec::new(),
            covered_branches: Vec::new(),
            proven_obligations: Vec::new(),
            proofs: Vec::new(),
        });
    }

    let mut measured = false;
    for record in records.iter().filter(|record| record.passed) {
        for artifact in record
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == "coverage")
        {
            let coverage: CoverageArtifact =
                serde_json::from_slice(&artifact.bytes).map_err(|err| {
                    BusError::Runtime(format!(
                        "cannot decode normalized coverage {}: {err}",
                        artifact.path
                    ))
                })?;
            let mapped = map_coverage_to_nodes(Some(&coverage), &graph)
                .map_err(|err| BusError::Intelligence(err.to_string()))?;
            measured = true;
            for node in mapped
                .into_iter()
                .filter(|node| node.measurement == CoverageMeasurement::Covered)
            {
                let flow = flows.get_mut(&node.node_id).ok_or_else(|| {
                    BusError::Intelligence(format!(
                        "coverage mapped unknown graph node {}",
                        node.node_id
                    ))
                })?;
                let test = format!("executor:{}@{}", record.executor, record.cwd);
                if !flow.tests.contains(&test) {
                    flow.tests.push(test);
                }
                if !flow.covered_nodes.contains(&node.node_id) {
                    flow.covered_nodes.push(node.node_id);
                }
            }
        }
    }
    if !measured {
        return Ok(None);
    }
    let snapshot = snapshot(revision, flows.into_values().collect())
        .map_err(|err| BusError::Runtime(err.to_string()))?;
    Ok(Some(snapshot))
}

fn head_coverage_graph(diff: &Value) -> Value {
    let mut nodes = Vec::new();
    nodes.extend(values_at(diff, "/nodes/added").iter().cloned());
    nodes.extend(
        values_at(diff, "/nodes/changed")
            .iter()
            .filter_map(|changed| changed.get("after").cloned()),
    );
    nodes.sort_by_key(graph_node_id);
    nodes.dedup_by(|left, right| graph_node_id(left) == graph_node_id(right));
    json!({"nodes": nodes})
}

fn values_at<'a>(value: &'a Value, pointer: &str) -> &'a [Value] {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn graph_node_id(node: &Value) -> Option<String> {
    node.get("id")
        .or_else(|| node.get("label"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn surface_labels(nodes: &[Value]) -> Vec<String> {
    nodes
        .iter()
        .filter(|node| {
            node.get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| {
                    ["endpoint", "route", "contract", "event"]
                        .iter()
                        .any(|surface| kind.contains(surface))
                })
        })
        .filter_map(|node| {
            node.get("label")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn execution_summary(
    run: &RunId,
    change: &str,
    revision: &RevisionId,
    range: &RevisionRange,
    requested_scope: &str,
    effective_scope: &str,
    evidence_policy: &str,
    outcome: &str,
    records: &[ExecutorRecord],
) -> Result<Vec<u8>, BusError> {
    let items: Vec<_> = records
        .iter()
        .map(|record| {
            json!({
                "executor": record.executor,
                "cwd": record.cwd,
                "selection": record.selection,
                "status_code": record.status_code,
                "passed": record.passed,
                "error": record.error,
                "stdout_bytes": record.stdout.len(),
                "stderr_bytes": record.stderr.len(),
                "artifacts": record.artifacts.iter().map(|artifact| json!({
                    "kind": artifact.kind,
                    "path": artifact.path,
                    "bytes": artifact.bytes.len(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::to_vec_pretty(&json!({
        "schema_v": 1,
        "run_id": run.as_str(),
        "change": change,
        "revision": revision.as_str(),
        "base": {"ref": range.base_ref, "commit": range.base_commit},
        "head": {"ref": range.head_ref, "commit": range.head_commit},
        "requested_scope": requested_scope,
        "effective_scope": effective_scope,
        "evidence_policy": evidence_policy,
        "outcome": outcome,
        "executors": items,
    }))
    .map_err(|err| BusError::Runtime(format!("cannot encode execution summary: {err}")))
}

const MAX_RUNNER_ARTIFACT_BYTES: u64 = 8 * 1024 * 1024;
const ARTIFACT_CLOCK_TOLERANCE: Duration = Duration::from_secs(2);

#[allow(clippy::too_many_lines)]
fn attach_normalized_artifacts(
    repo: &Path,
    cwd: &Path,
    started: SystemTime,
    record: &mut ExecutorRecord,
) {
    if record.executor == "go-test" && !record.stdout.is_empty() {
        match std::str::from_utf8(&record.stdout)
            .map_err(|err| format!("go-json output is not UTF-8: {err}"))
            .and_then(|text| parse_go_json(text).map_err(|err| err.to_string()))
            .and_then(|run| {
                serde_json::to_vec_pretty(&run)
                    .map_err(|err| format!("cannot encode normalized go-json: {err}"))
            }) {
            Ok(bytes) => record.artifacts.push(ProducedArtifact {
                kind: "normalized-test-run".into(),
                path: "stdout#normalized".into(),
                bytes,
            }),
            Err(err) => set_record_error(record, err),
        }
    }

    for (path, kind) in runner_artifact_candidates(cwd) {
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) | Err(_) => continue,
        };
        if !artifact_is_fresh(&metadata, started) {
            continue;
        }
        if metadata.len() > MAX_RUNNER_ARTIFACT_BYTES {
            set_record_error(
                record,
                format!(
                    "runner artifact {} exceeds {} bytes",
                    path.display(),
                    MAX_RUNNER_ARTIFACT_BYTES
                ),
            );
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                set_record_error(
                    record,
                    format!("cannot read runner artifact {}: {err}", path.display()),
                );
                continue;
            }
        };
        let display_path = relative_or_display(cwd, &path);
        record.artifacts.push(ProducedArtifact {
            kind: kind.into(),
            path: display_path.clone(),
            bytes: bytes.clone(),
        });

        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(err) => {
                set_record_error(
                    record,
                    format!("runner artifact {} is not UTF-8: {err}", path.display()),
                );
                continue;
            }
        };
        let normalized = match kind {
            "junit" => parse_junit(text)
                .and_then(|run| {
                    serde_json::to_vec_pretty(&run).map_err(|err| {
                        wvq_runtime::RuntimeError::Malformed {
                            kind: "normalized-test-run".into(),
                            message: err.to_string(),
                        }
                    })
                })
                .map(|bytes| ("normalized-test-run", bytes)),
            "lcov" => parse_lcov(text)
                .map(|mut coverage| {
                    normalize_coverage_paths(repo, cwd, &mut coverage);
                    coverage
                })
                .and_then(|coverage| {
                    serde_json::to_vec_pretty(&coverage).map_err(|err| {
                        wvq_runtime::RuntimeError::Malformed {
                            kind: "coverage".into(),
                            message: err.to_string(),
                        }
                    })
                })
                .map(|bytes| ("coverage", bytes)),
            "go-coverprofile" => parse_go_coverprofile(text)
                .map(|mut coverage| {
                    normalize_coverage_paths(repo, cwd, &mut coverage);
                    coverage
                })
                .and_then(|coverage| {
                    serde_json::to_vec_pretty(&coverage).map_err(|err| {
                        wvq_runtime::RuntimeError::Malformed {
                            kind: "coverage".into(),
                            message: err.to_string(),
                        }
                    })
                })
                .map(|bytes| ("coverage", bytes)),
            _ => continue,
        };
        match normalized {
            Ok((normalized_kind, bytes)) => record.artifacts.push(ProducedArtifact {
                kind: normalized_kind.into(),
                path: format!("{display_path}#normalized"),
                bytes,
            }),
            Err(err) => set_record_error(record, err.to_string()),
        }
    }
}

fn normalize_coverage_paths(repo: &Path, cwd: &Path, coverage: &mut CoverageArtifact) {
    let repo_path = normalize_path(&repo.to_string_lossy());
    let cwd_path = normalize_path(&cwd.to_string_lossy());
    let cwd_prefix = cwd
        .strip_prefix(repo)
        .ok()
        .map(|path| normalize_path(&path.to_string_lossy()))
        .filter(|path| !path.is_empty());
    let go_module = read_go_module(cwd);
    for file in &mut coverage.files {
        let mut path = normalize_path(&file.path);
        if let Some(relative) = path.strip_prefix(&format!("{repo_path}/")) {
            path = String::from(relative);
        } else if let Some(relative) = path.strip_prefix(&format!("{cwd_path}/")) {
            path = cwd_prefix.as_ref().map_or_else(
                || relative.to_owned(),
                |prefix| format!("{prefix}/{relative}"),
            );
        } else {
            if let Some(module) = &go_module
                && let Some(relative) = path.strip_prefix(&format!("{module}/"))
            {
                path = String::from(relative);
            }
            path = String::from(path.trim_start_matches("./"));
            if let Some(prefix) = &cwd_prefix
                && !path.starts_with(&format!("{prefix}/"))
            {
                path = format!("{prefix}/{path}");
            }
        }
        file.path = path;
    }
}

fn read_go_module(cwd: &Path) -> Option<String> {
    std::fs::read_to_string(cwd.join("go.mod"))
        .ok()?
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("module ").map(str::trim))
        .filter(|module| !module.is_empty())
        .map(ToOwned::to_owned)
}

fn runner_artifact_candidates(cwd: &Path) -> Vec<(PathBuf, &'static str)> {
    let candidates = [
        (".weavatrix-quality/go-cover.out", "go-coverprofile"),
        (".weavatrix-quality/junit.xml", "junit"),
        ("junit.xml", "junit"),
        ("test-results/junit.xml", "junit"),
        ("reports/junit.xml", "junit"),
        ("coverage/junit.xml", "junit"),
        ("lcov.info", "lcov"),
        ("coverage.lcov", "lcov"),
        ("coverage/lcov.info", "lcov"),
        ("coverage/lcov-report/lcov.info", "lcov"),
    ];
    candidates
        .into_iter()
        .map(|(path, kind)| (cwd.join(path), kind))
        .collect()
}

fn artifact_is_fresh(metadata: &std::fs::Metadata, started: SystemTime) -> bool {
    let threshold = started
        .checked_sub(ARTIFACT_CLOCK_TOLERANCE)
        .unwrap_or(UNIX_EPOCH);
    metadata
        .modified()
        .is_ok_and(|modified| modified >= threshold)
}

fn set_record_error(record: &mut ExecutorRecord, message: impl Into<String>) {
    let message = message.into();
    record.passed = false;
    record.error = Some(match record.error.take() {
        Some(existing) => format!("{existing}; {message}"),
        None => message,
    });
}

fn stdout_kind(executor: &str) -> &'static str {
    if executor == "go-test" {
        "go-json"
    } else {
        "stdout"
    }
}

fn relative_or_display(repo: &Path, path: &Path) -> String {
    path.strip_prefix(repo)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .map_or_else(
            || path.display().to_string(),
            |relative| relative.display().to_string(),
        )
}

fn evidence_from_bytes(handle: &str, bytes: &[u8]) -> EvidenceReply {
    let hash = sha256_hex(bytes);
    let inline_text = if bytes.len() <= INLINE_LIMIT {
        std::str::from_utf8(bytes).ok().map(ToOwned::to_owned)
    } else {
        None
    };
    EvidenceReply {
        handle: handle.to_owned(),
        kind: "bytes".into(),
        byte_len: bytes.len() as u64,
        content_hash: Some(hash),
        inline_text,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("wvq-{label}-{nanos}"));
            std::fs::create_dir_all(&path).expect("temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn record(executor: &str) -> ExecutorRecord {
        ExecutorRecord {
            executor: executor.into(),
            cwd: ".".into(),
            selection: None,
            status_code: Some(0),
            passed: true,
            error: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            artifacts: Vec::new(),
        }
    }

    #[test]
    fn fresh_junit_and_lcov_are_preserved_and_normalized() {
        let root = TempDir::new("runner-artifacts");
        std::fs::create_dir_all(root.0.join("coverage")).unwrap();
        let started = SystemTime::now();
        std::fs::write(
            root.0.join("junit.xml"),
            "<testsuite name=\"suite\"><testcase name=\"works\"/></testsuite>",
        )
        .unwrap();
        std::fs::write(
            root.0.join("coverage/lcov.info"),
            "SF:src/lib.rs\nDA:1,1\nDA:2,0\nend_of_record\n",
        )
        .unwrap();
        let mut record = record("npm-test");

        attach_normalized_artifacts(&root.0, &root.0, started, &mut record);

        assert!(record.passed);
        assert!(record.error.is_none());
        assert_eq!(
            record
                .artifacts
                .iter()
                .map(|artifact| artifact.kind.as_str())
                .collect::<Vec<_>>(),
            ["junit", "normalized-test-run", "lcov", "coverage"]
        );
    }

    #[test]
    fn malformed_fresh_evidence_fails_the_record() {
        let root = TempDir::new("bad-runner-artifact");
        let started = SystemTime::now();
        std::fs::write(root.0.join("junit.xml"), "<testsuite><testcase").unwrap();
        let mut record = record("npm-test");

        attach_normalized_artifacts(&root.0, &root.0, started, &mut record);

        assert!(!record.passed);
        assert!(
            record
                .error
                .as_deref()
                .is_some_and(|message| message.contains("truncated junit"))
        );
        assert_eq!(record.artifacts.len(), 1, "raw evidence remains auditable");
    }

    #[test]
    fn artifact_from_before_the_run_is_not_reused() {
        let root = TempDir::new("stale-runner-artifact");
        std::fs::write(
            root.0.join("junit.xml"),
            "<testsuite><testcase name=\"stale\"/></testsuite>",
        )
        .unwrap();
        let started = SystemTime::now() + Duration::from_secs(10);
        let mut record = record("npm-test");

        attach_normalized_artifacts(&root.0, &root.0, started, &mut record);

        assert!(record.artifacts.is_empty());
    }

    #[test]
    fn live_graph_diff_and_coverage_build_revision_bound_protection() {
        let diff = json!({
            "counts": {
                "nodes_added": 0,
                "nodes_removed": 0,
                "nodes_changed": 1,
                "edges_added": 0,
                "edges_removed": 0
            },
            "nodes": {
                "added": [],
                "removed": [],
                "changed": [{
                    "before": {"id": "symbol:old", "span": {"file": "src/lib.rs", "start_line": 1, "end_line": 2}},
                    "after": {"id": "symbol:add", "span": {"file": "src/lib.rs", "start_line": 1, "end_line": 2}}
                }]
            },
            "edges": {"added": [], "removed": []}
        });
        let impact =
            live_impacted_surface(&diff, &json!({"impacted_nodes": [{"id": "symbol:caller"}]}))
                .unwrap();
        assert!(impact.base_only.contains(&"symbol:old".into()));
        assert!(impact.head_only.contains(&"symbol:add".into()));
        assert!(impact.head_only.contains(&"symbol:caller".into()));

        let coverage = CoverageArtifact {
            files: vec![wvq_runtime::FileCoverage {
                path: "src/lib.rs".into(),
                covered: vec![wvq_runtime::LineRange { start: 1, end: 2 }],
                uncovered: Vec::new(),
            }],
        };
        let mut record = record("cargo-test");
        record.artifacts.push(ProducedArtifact {
            kind: "coverage".into(),
            path: "coverage#normalized".into(),
            bytes: serde_json::to_vec(&coverage).unwrap(),
        });
        let revision = RevisionId::new("revision-1").unwrap();
        let protection = live_protection_snapshot(&revision, &diff, &[record])
            .unwrap()
            .unwrap();
        let flow = protection.flow("symbol:add").unwrap();
        assert_eq!(flow.revision, "revision-1");
        assert_eq!(flow.covered_nodes, ["symbol:add"]);
        assert_eq!(flow.tests, ["executor:cargo-test@."]);
    }

    #[test]
    fn debt_policy_loads_active_exceptions_and_rejects_expired_ones() {
        let root = TempDir::new("debt-policy");
        std::fs::create_dir_all(root.0.join(".weavatrix-quality")).unwrap();
        std::fs::write(
            root.0.join(".weavatrix-quality/config.yaml"),
            "quality_policy_v: 1\nratchet:\n  mode: no_new_debt\n  exceptions:\n    - fingerprint: active-id\n      reason: tracked cleanup\n      expires: 2999-12-31\n    - fingerprint: expired-id\n      reason: old waiver\n      expires: 2000-01-01\n",
        )
        .unwrap();

        let exceptions = load_debt_exceptions(&root.0).unwrap();

        assert_eq!(exceptions.active, BTreeSet::from(["active-id".into()]));
        assert_eq!(exceptions.notes.len(), 1);
        assert!(exceptions.notes[0].contains("expired-id"));
    }

    #[test]
    fn current_utc_date_uses_iso_ordering() {
        let today = utc_date();
        assert!(valid_iso_date(&today));
    }
}
