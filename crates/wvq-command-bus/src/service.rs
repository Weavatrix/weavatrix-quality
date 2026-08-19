//! Domain facade. CLI and MCP call this; they do not reimplement policy.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use sha2::{Digest, Sha256};
use thiserror::Error;
use wvq_domain::{ProofId, RevisionId};
use wvq_intelligence::{
    DebtBaseline, ObligationNeed, SelectionInput, classify_debt, select_minimal_plan,
};
use wvq_proof::{AssemblyInput, ExecutionEvidence, ProofVerdict, assemble};
use wvq_spec::{
    ObligationKind, OpenSpecChange, RequirementOp, RiskLevel, SpecError, TestObligation,
    compile_obligations, load_quality_contract, read_change, seal,
};

use crate::commands::{
    ChangesCommand, Command, ContextCommand, DebtCommand, EvidenceCommand, ExplainCommand,
    PlanCommand, RunCommand, SelectCommand, SpecCommand, StatusCommand, VerifyCommand,
};
use crate::replies::{
    ChangesReply, ContextReply, DebtReply, EvidenceReply, ExplainReply, INLINE_LIMIT, PlanReply,
    ProofSummary, Reply, RunReply, SelectReply, SpecSealReply, SpecValidateReply, StatusReply,
    VerifyReply, bound_items,
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
    /// Record a bounded run. No arbitrary shell.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] on unknown scope/policy or missing change.
    fn run(&self, cmd: &RunCommand) -> Result<RunReply, BusError>;
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
        validate_scope(&cmd.scope)?;
        validate_evidence_policy(&cmd.evidence_policy)?;
        let mut inner = self.lock();
        inner.run_executed = true;
        let state = RunState {
            id: "run-fake".into(),
            status: "complete".into(),
            handles: inner.evidence.keys().cloned().collect(),
        };
        inner.last_run = Some(state.clone());
        Ok(RunReply {
            run_id: state.id,
            change: cmd.change.clone(),
            scope: cmd.scope.clone(),
            status: "complete".into(),
            executed: true,
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
                handles: run.handles.clone(),
            }),
            (Some(want), None) => Err(BusError::NotFound(format!("run {want}"))),
            (None, None) => Ok(StatusReply {
                run_id: None,
                status: "idle".into(),
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
        let _ = cmd;
        Ok(empty_debt())
    }

    fn select(&self, cmd: &SelectCommand) -> Result<SelectReply, BusError> {
        let _ = cmd;
        Ok(SelectReply {
            algorithm: "greedy-weighted-set-cover".into(),
            selected: Vec::new(),
            uncovered_mandatory: vec!["others-visible".into()],
            explanations: Vec::new(),
            executed: false,
        })
    }

    fn changes(&self, cmd: &ChangesCommand) -> Result<ChangesReply, BusError> {
        let _ = cmd;
        Ok(ChangesReply {
            changes: vec!["sankey-others".into()],
        })
    }
}

/// Filesystem-backed service: real `OpenSpec` compile/seal; no subprocesses.
#[derive(Debug)]
pub struct LiveService {
    repo: PathBuf,
    state: Mutex<Option<RunState>>,
}

impl LiveService {
    /// `repo` is the repository root that contains `openspec/`.
    #[must_use]
    pub fn new(repo: impl AsRef<Path>) -> Self {
        Self {
            repo: repo.as_ref().to_path_buf(),
            state: Mutex::new(None),
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
}

struct Compiled {
    change: String,
    spec: OpenSpecChange,
    obligations: Vec<TestObligation>,
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
        let gaps = compiled
            .obligations
            .iter()
            .map(|item| format!("{}: no runtime evidence", item.id))
            .collect();
        Ok(PlanReply {
            change: compiled.change,
            requirements,
            obligations,
            risk,
            existing_proofs: Vec::new(),
            gaps,
            checks: deterministic_checks(),
            executed: false,
        })
    }

    fn run(&self, cmd: &RunCommand) -> Result<RunReply, BusError> {
        validate_scope(&cmd.scope)?;
        validate_evidence_policy(&cmd.evidence_policy)?;
        let compiled = self.compiled(&cmd.change)?;
        let state = RunState {
            id: format!("run-{}", compiled.change),
            status: "complete".into(),
            handles: Vec::new(),
        };
        *self.lock() = Some(state.clone());
        Ok(RunReply {
            run_id: state.id,
            change: compiled.change,
            scope: cmd.scope.clone(),
            status: "complete".into(),
            executed: true,
            artifact_handles: Vec::new(),
        })
    }

    fn status(&self, cmd: &StatusCommand) -> Result<StatusReply, BusError> {
        match (&cmd.run_id, &*self.lock()) {
            (Some(want), Some(run)) if want != &run.id => {
                Err(BusError::NotFound(format!("run {want}")))
            }
            (_, Some(run)) => Ok(StatusReply {
                run_id: Some(run.id.clone()),
                status: run.status.clone(),
                handles: run.handles.clone(),
            }),
            (Some(want), None) => Err(BusError::NotFound(format!("run {want}"))),
            (None, None) => Ok(StatusReply {
                run_id: None,
                status: "idle".into(),
                handles: Vec::new(),
            }),
        }
    }

    fn verify(&self, cmd: &VerifyCommand) -> Result<VerifyReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let contract = load_quality_contract(&self.repo, &compiled.change)?;
        let oracle = seal(&contract, &compiled.obligations, &compiled.spec)?;
        let revision =
            RevisionId::new("current").map_err(|err| BusError::Identity(err.to_string()))?;
        let mut proofs = Vec::new();
        let mut verdicts = Vec::new();
        for obligation in &compiled.obligations {
            let id = ProofId::new(format!("proof-{}", obligation.id))
                .map_err(|err| BusError::Identity(err.to_string()))?;
            let assembled = assemble(AssemblyInput {
                id,
                requirement: obligation.requirement.clone(),
                scenario: obligation.scenario.clone(),
                obligation: obligation.id.clone(),
                oracle_seal: oracle.id.clone(),
                revision: revision.clone(),
                program: None,
                run: None,
                observations: Vec::new(),
                artifacts: Vec::new(),
                required_evidence: obligation.required_evidence.clone(),
                execution: ExecutionEvidence::Absent,
                spec_ambiguous: false,
                quality_debt: Vec::new(),
                mutation: None,
            });
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
        Err(BusError::NotFound(format!("id {}", cmd.id)))
    }

    fn evidence(&self, cmd: &EvidenceCommand) -> Result<EvidenceReply, BusError> {
        Err(BusError::NotFound(format!("handle {}", cmd.handle)))
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
        let delta = classify_debt(&[], &[], &DebtBaseline::default());
        Ok(DebtReply {
            existing: delta.existing.len() as u64,
            new: delta.new.len() as u64,
            fixed: delta.fixed.len() as u64,
            returned: delta.returned.len() as u64,
            excepted: delta.excepted.len() as u64,
            findings: Vec::new(),
        })
    }

    fn select(&self, cmd: &SelectCommand) -> Result<SelectReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let obligations = compiled
            .obligations
            .iter()
            .map(|item| ObligationNeed {
                id: item.id.to_string(),
                high_risk: matches!(item.risk, RiskLevel::High | RiskLevel::Critical),
            })
            .collect();
        let plan = select_minimal_plan(SelectionInput {
            candidates: Vec::new(),
            obligations,
        });
        Ok(SelectReply {
            algorithm: plan.algorithm.to_owned(),
            selected: plan.selected.iter().map(|item| item.id.clone()).collect(),
            uncovered_mandatory: plan.uncovered_mandatory,
            explanations: plan
                .selected
                .iter()
                .map(|item| item.explanation.clone())
                .collect(),
            executed: false,
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

fn empty_debt() -> DebtReply {
    DebtReply {
        existing: 0,
        new: 0,
        fixed: 0,
        returned: 0,
        excepted: 0,
        findings: Vec::new(),
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
