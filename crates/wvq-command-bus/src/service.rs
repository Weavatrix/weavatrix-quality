//! Domain facade. CLI and MCP call this; they do not reimplement policy.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
use wvq_domain::{
    ArtifactId, ContentHash, OracleSealId, ProgramId, ProofId, RevisionId, RunId, Severity,
};
use wvq_intelligence::{
    CodeEvidenceProvider, CoverageMeasurement, GraphDelta, ObligationNeed, SelectionInput,
    SurfaceDelta, TestCandidate, WeavatrixProvider, impacted_surface, map_coverage_to_nodes,
    select_minimal_plan,
};
use wvq_proof::{
    AiAxis, AiBudget, AiCallKind, AiCostFirewall, AiUsage, AssemblyInput, AxisState,
    ChangeQualityVerdict, CodeDelta, DebtAxis, DebtItem, DeltaContext, DeltaFindingRef,
    DeltaTriangleAxis, ExecutionEvidence, FailureEvidence, FlakeClass, FlowProtection, FlowView,
    HealEdit, Limitation, LocalModelConfig, LocalModelRequest, OracleReplacementReview,
    ProofOutcome, ProofVerdict, ProtectionAxis, ProtectionCheckInput, ProtectionDelta,
    ProtectionDeltaState, ProtectionFinding, ProtectionPolicy, ProtectionSnapshot, ProtectionView,
    SpecDelta, StabilityAxis, TestChange, TestLineageView, TimingBucket, UiFindingRef,
    UiIntegrityAxis, VerdictInputs, apply_heal, assemble, call_local_model, compose,
    debt_rule_blocks, fingerprint_id, gate_protection, join_triangle, protection_delta,
    snapshot_with_executed_tests, summarise, triage,
};
use wvq_runtime::{
    AxisDelta, BehaviorDelta, BehaviorState, BrowserAssertionStatus, BrowserProgramRun,
    BrowserRunConfig, BrowserViewport, CaptureWhen, CoverageArtifact, DiffAxis, ExecutionResult,
    ExecutorRegistry, ExecutorTarget, NormalizedTestRun, PrepareRequest, ProgramOracle,
    StructuredView, TestAction, TestProgram, TestStatus, UiCollectionConfig, behavior_delta,
    default_limits, discover_executor_targets, parse_cargo_test, parse_go_coverprofile,
    parse_go_json, parse_junit, parse_lcov, run_browser_program, run_browser_program_at,
};
use wvq_spec::{
    EvidenceKind, ObligationKind, OpenSpecChange, RequirementOp, RiskLevel, SpecError,
    TestObligation, compile_obligations, load_quality_contract, read_change, seal,
};
use wvq_spec_recovery::{
    CandidateRequirement, CandidateShape, CodeDeltaSummary, CommitFacts, EvidenceSource,
    IntentEvidence, NarrativeInput, PublicSurfaceDelta, RecoveryDesk, RecoveryInput,
    TestIntentSummary, TestsDelta, VerifyContext, cluster, narrate,
};
use wvq_store::{
    HistoricalTestCandidate, Store, StoreError, StoredAiUsage, StoredProof, StoredRun,
    StoredRunItem, StoredSelectionAudit, StoredTestCaseIdentity, StoredTestCaseResult,
};
use wvq_ui::{
    LayoutSnapshot, ResponsiveProbe, UiFindingState, UiIntegrityDelta, UiIntegrityFinding,
    UiIntegrityPolicy, UiIntegritySnapshot, detect as detect_ui, next_responsive_probe,
    parse_policy as parse_ui_policy, ratchet as ratchet_ui, responsive_failure_intervals,
    responsive_probe_plan,
};

use crate::commands::{
    AuthorDraftCommand, AuthorHealCommand, AuthorHealEdit, AuthorPreviewCommand,
    AuthorPromoteCommand, AuthorValidateCommand, ChangesCommand, Command, ContextCommand,
    DebtCommand, EvidenceCommand, ExplainCommand, ModelCommand, PlanCommand, RecoveryCommand,
    RunCommand, SelectCommand, SpecCommand, StatusCommand, VerifyCommand,
};
use crate::replies::{
    AuthorDraftReply, AuthorHealReply, AuthorModelUsage, AuthorPreviewReply, AuthorPromoteReply,
    AuthorValidateReply, AuthoringObligation, ChangesReply, ContextReply, DebtReply, EvidenceReply,
    ExplainReply, INLINE_LIMIT, ModelReply, PlanReply, ProofSummary, RecoveryReply, Reply,
    RunReply, SelectReply, SelectionAuditReply, SpecSealReply, SpecValidateReply, StatusReply,
    VerifyReply, bound_items, estimate_tokens,
};

/// CAS artifact kind holding the base/head UI-integrity ratchet for one run.
const UI_INTEGRITY_DELTA_KIND: &str = "ui-integrity-delta";

/// CAS artifact kind holding live same-program Spec x Code x Behavior evidence.
const DELTA_TRIANGLE_KIND: &str = "delta-triangle";

/// CAS artifact kind for the exact expectation replacement a QA reviewed.
const ORACLE_REPLACEMENT_KIND: &str = "oracle-replacement-proposal";

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
    /// Caller-supplied command or candidate failed strict validation.
    #[error("invalid input: {0}")]
    InvalidInput(String),
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
    /// Compare an impacted run with its defensive full run and persist learning.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when runs are missing, incomparable, or evidence is malformed.
    fn audit_selection(
        &self,
        impacted_run: &str,
        full_run: &str,
    ) -> Result<SelectionAuditReply, BusError>;
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
    /// Build a bounded revision- and seal-bound browser-test authoring packet.
    ///
    /// The optional model call is explicit and charged through the AI Cost Firewall.
    /// This method never persists a candidate or changes an oracle.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when revision, graph, intent, budget, or model evidence is invalid.
    fn author_draft(&self, cmd: &AuthorDraftCommand) -> Result<AuthorDraftReply, BusError>;
    /// Validate one candidate `TestProgram` against an existing `OracleSeal`.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] on malformed IR, unknown obligations, or missing sealed predicates.
    fn author_validate(&self, cmd: &AuthorValidateCommand)
    -> Result<AuthorValidateReply, BusError>;
    /// Execute a validated candidate through actual Playwright and return evidence handles.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] on invalid IR/runtime/revision evidence.
    fn author_preview(&self, cmd: &AuthorPreviewCommand) -> Result<AuthorPreviewReply, BusError> {
        self.author_preview_controlled(cmd, Arc::new(AtomicBool::new(false)))
    }
    /// Execute an authoring preview with transport-owned cooperative cancellation.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`QualityService::author_preview`].
    fn author_preview_controlled(
        &self,
        cmd: &AuthorPreviewCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorPreviewReply, BusError> {
        let _ = cancel;
        self.author_preview(cmd)
    }
    /// Persist a passing preview as revision 1 of a canonical program.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when the preview, program, seal, or repository revision differs.
    fn author_promote(&self, cmd: &AuthorPromoteCommand) -> Result<AuthorPromoteReply, BusError>;
    /// Apply locator/wait-only repair, replay it, and append a version only on pass.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] for stale versions, changed seals, illegal edits, or runtime failures.
    fn author_heal(&self, cmd: &AuthorHealCommand) -> Result<AuthorHealReply, BusError> {
        self.author_heal_controlled(cmd, Arc::new(AtomicBool::new(false)))
    }
    /// Safe-healing replay with transport-owned cooperative cancellation.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`QualityService::author_heal`].
    fn author_heal_controlled(
        &self,
        cmd: &AuthorHealCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorHealReply, BusError> {
        let _ = cancel;
        self.author_heal(cmd)
    }
    /// Known `OpenSpec` changes.
    ///
    /// # Errors
    ///
    /// Returns [`BusError::NotFound`] when `openspec/changes` cannot be read.
    fn changes(&self, cmd: &ChangesCommand) -> Result<ChangesReply, BusError>;
    /// Build a revision-bound recovery packet without sealing recovered intent.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when revision or graph evidence is incomplete.
    fn recovery(&self, cmd: &RecoveryCommand) -> Result<RecoveryReply, BusError>;
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
        Command::Verify(cmd) => service
            .verify(&cmd)
            .map(|reply| Reply::Verify(Box::new(reply))),
        Command::Explain(cmd) => service.explain(&cmd).map(Reply::Explain),
        Command::Evidence(cmd) => service.evidence(&cmd).map(Reply::Evidence),
        Command::SpecValidate(cmd) => service.spec_validate(&cmd).map(Reply::SpecValidate),
        Command::SpecSeal(cmd) => service.spec_seal(&cmd).map(Reply::SpecSeal),
        Command::Debt(cmd) => service.debt(&cmd).map(Reply::Debt),
        Command::Select(cmd) => service.select(&cmd).map(Reply::Select),
        Command::Model(cmd) => service.model(&cmd).map(Reply::Model),
        Command::AuthorDraft(cmd) => service.author_draft(&cmd).map(Reply::AuthorDraft),
        Command::AuthorValidate(cmd) => service.author_validate(&cmd).map(Reply::AuthorValidate),
        Command::AuthorPreview(cmd) => service.author_preview(&cmd).map(Reply::AuthorPreview),
        Command::AuthorPromote(cmd) => service.author_promote(&cmd).map(Reply::AuthorPromote),
        Command::AuthorHeal(cmd) => service.author_heal(&cmd).map(Reply::AuthorHeal),
        Command::Changes(cmd) => service.changes(&cmd).map(Reply::Changes),
        Command::Recovery(cmd) => service
            .recovery(&cmd)
            .map(|reply| Reply::Recovery(Box::new(reply))),
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
            base_commit: "fake-base-commit".into(),
            head_commit: "fake-head-commit".into(),
            merge_base: "fake-merge-base".into(),
            requested_scope: cmd.scope.clone(),
            scope: cmd.scope.clone(),
            scope_reason: format!("{} scope requested by caller", cmd.scope),
            status: "complete".into(),
            executed: true,
            outcome: state.outcome,
            selected_test_count: 1,
            available_test_count: 2,
            executor_invocations: 0,
            browser_programs: 0,
            behavior_state_count: 0,
            new_behavior_state_count: 0,
            behavior_edge_count: 0,
            new_behavior_edge_count: 0,
            recorded_test_count: 0,
            failed_test_count: 0,
            flaky_test_count: 0,
            unknown_failure_count: 0,
            artifact_handles: state.handles,
        })
    }

    fn audit_selection(
        &self,
        impacted_run: &str,
        full_run: &str,
    ) -> Result<SelectionAuditReply, BusError> {
        if impacted_run.is_empty() || full_run.is_empty() {
            return Err(BusError::Identity("selection audit run id is empty".into()));
        }
        Ok(SelectionAuditReply {
            audit_id: format!("audit-{impacted_run}-{full_run}"),
            status: "unmeasured".into(),
            missed_failure_count: 0,
            learned_test_count: 0,
            evidence_handle: None,
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

    fn author_draft(&self, cmd: &AuthorDraftCommand) -> Result<AuthorDraftReply, BusError> {
        validate_authoring_budget(cmd.token_budget)?;
        let model_usage = cmd.use_model.then(|| AuthorModelUsage {
            model: "fake-local-model".into(),
            input_tokens: 1,
            output_tokens: 1,
            cost_micros: 0,
        });
        Ok(AuthorDraftReply {
            change: cmd.change.clone(),
            revision: "fake-revision".into(),
            base: cmd.base.clone(),
            head: cmd.head.clone(),
            changed_files: vec!["src/widget.ts".into()],
            context: vec!["changed file src/widget.ts".into()],
            obligations: vec![AuthoringObligation {
                id: "others-visible".into(),
                requirement: "sankey.visual-limit".into(),
                scenario: "overflow-grouped".into(),
                kind: "behavioral".into(),
                risk: "high".into(),
                condition: None,
                expected: Some(json!({"kind": "visible", "target": {"test_id": "others"}})),
                required_evidence: vec!["dom".into()],
            }],
            truncated: false,
            tokens_used: 32,
            token_budget: cmd.token_budget,
            candidate: None,
            model_usage,
        })
    }

    fn author_validate(
        &self,
        cmd: &AuthorValidateCommand,
    ) -> Result<AuthorValidateReply, BusError> {
        let id = cmd
            .program
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| BusError::InvalidInput("authoring candidate omitted id".into()))?;
        Ok(AuthorValidateReply {
            change: cmd.change.clone(),
            seal_id: "oseal-fake".into(),
            program_id: id.into(),
            program: cmd.program.clone(),
            obligations: vec!["others-visible".into()],
            valid: true,
            persisted: false,
        })
    }

    fn author_preview_controlled(
        &self,
        cmd: &AuthorPreviewCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorPreviewReply, BusError> {
        let _ = cancel;
        let validated = self.author_validate(&AuthorValidateCommand {
            change: cmd.change.clone(),
            program: cmd.program.clone(),
        })?;
        Ok(AuthorPreviewReply {
            preview_id: format!("preview-{}", validated.program_id),
            change: validated.change,
            revision: "fake-revision".into(),
            program_id: validated.program_id,
            passed: true,
            asserted: validated.obligations,
            contradicted: Vec::new(),
            failure: None,
            observation_handles: vec!["artifact-fake-author-observation-0".into()],
            screenshot_handles: if cmd.screenshot {
                vec!["artifact-fake-author-screenshot-0".into()]
            } else {
                Vec::new()
            },
            trace_handle: cmd.trace.then(|| "artifact-fake-author-trace".into()),
            program_persisted: false,
        })
    }

    fn author_promote(&self, cmd: &AuthorPromoteCommand) -> Result<AuthorPromoteReply, BusError> {
        let validated = self.author_validate(&AuthorValidateCommand {
            change: cmd.change.clone(),
            program: cmd.program.clone(),
        })?;
        Ok(AuthorPromoteReply {
            change: validated.change,
            revision: "fake-revision".into(),
            seal_id: validated.seal_id,
            program_id: validated.program_id,
            program_revision: 1,
            persisted: true,
            created: true,
        })
    }

    fn author_heal_controlled(
        &self,
        cmd: &AuthorHealCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorHealReply, BusError> {
        let _ = cancel;
        if cmd.program_id.trim().is_empty() || cmd.expected_program_revision == 0 {
            return Err(BusError::InvalidInput(
                "healing requires a program id and positive expected revision".into(),
            ));
        }
        Ok(AuthorHealReply {
            preview_id: format!("preview-heal-{}", cmd.program_id),
            change: cmd.change.clone(),
            revision: "fake-revision".into(),
            seal_id: "oseal-fake".into(),
            program_id: cmd.program_id.clone(),
            previous_program_revision: cmd.expected_program_revision,
            program_revision: Some(cmd.expected_program_revision.saturating_add(1)),
            passed: true,
            asserted: vec!["others-visible".into()],
            contradicted: Vec::new(),
            failure: None,
            observation_handles: vec!["artifact-fake-heal-observation-0".into()],
            screenshot_handles: if cmd.screenshot {
                vec!["artifact-fake-heal-screenshot-0".into()]
            } else {
                Vec::new()
            },
            trace_handle: cmd.trace.then(|| "artifact-fake-heal-trace".into()),
            persisted: true,
            created: true,
        })
    }

    fn changes(&self, cmd: &ChangesCommand) -> Result<ChangesReply, BusError> {
        let _ = cmd;
        Ok(ChangesReply {
            changes: vec!["sankey-others".into()],
        })
    }

    fn recovery(&self, cmd: &RecoveryCommand) -> Result<RecoveryReply, BusError> {
        Err(BusError::NotFound(format!(
            "fake recovery is not configured for {}",
            cmd.change
        )))
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

fn canonical_repo_path(repo: &Path) -> PathBuf {
    let canonical = std::fs::canonicalize(repo).unwrap_or_else(|_| repo.to_path_buf());
    strip_windows_verbatim_prefix(&canonical)
}

#[cfg(not(windows))]
fn strip_windows_verbatim_prefix(path: &Path) -> PathBuf {
    path.to_path_buf()
}

#[cfg(windows)]
fn strip_windows_verbatim_prefix(path: &Path) -> PathBuf {
    use std::path::{Component, Prefix};

    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return path.to_path_buf();
    };
    let mut normalized = match prefix.kind() {
        Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:\\", char::from(drive))),
        Prefix::VerbatimUNC(server, share) => {
            let mut root = PathBuf::from(r"\\");
            root.push(server);
            root.push(share);
            root
        }
        _ => return path.to_path_buf(),
    };
    for component in components {
        if !matches!(component, Component::RootDir | Component::CurDir) {
            normalized.push(component.as_os_str());
        }
    }
    normalized
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
            repo: canonical_repo_path(repo.as_ref()),
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
            repo: canonical_repo_path(repo.as_ref()),
            state: Mutex::new(None),
            executors,
            executor_init_error: None,
        }
    }

    /// Build a live brownfield recovery desk from an exact Git range and
    /// revision-bound Weavatrix evidence. Recovered candidates remain proposals;
    /// this method never seals them.
    ///
    /// # Errors
    ///
    /// Fails closed when refs, graph evidence, or Git provenance are unavailable.
    pub fn recovery_desk(
        &self,
        change: &str,
        base: &str,
        head: &str,
    ) -> Result<RecoveryDesk, BusError> {
        let range = self.revision_range(base, head)?;
        let revision = self.revision()?;
        let diff = self.weavatrix_operation(
            &revision,
            "graph_diff",
            &json!({
                "base_ref": range.merge_base,
                "detail": "edges",
                "max_results": 100_000
            }),
        )?;
        ensure_complete_diff(&diff)?;
        let files = changed_files(&self.repo, &range)?;
        let (code_delta, surfaces) = recovery_code_delta(&diff);
        if files.is_empty() && code_delta.changed_symbols.is_empty() {
            return Err(BusError::Intelligence(format!(
                "revision range `{base}` -> `{head}` contains no recoverable change"
            )));
        }

        let head_revision = if head == "WORKTREE" {
            format!("WORKTREE@{}", revision.as_str())
        } else {
            range.head_commit.clone()
        };
        let existing_requirements = recovery_existing_requirements(&self.repo, change)?;
        let evidence = recovery_evidence(
            &self.repo,
            &range,
            &code_delta,
            &files,
            &existing_requirements,
        )?;
        let commits = recovery_commits(
            &self.repo,
            &range,
            &head_revision,
            &code_delta.components,
            !files.is_empty(),
        )?;
        let clusters = cluster(&commits);
        let narrative = narrate(NarrativeInput {
            change_cluster: change.to_owned(),
            base_revision: range.merge_base.clone(),
            head_revision,
            evidence: evidence.clone(),
            code_delta: code_delta.clone(),
            tests_delta: files.tests_delta(),
            behavior_delta: Vec::new(),
        });
        let recover_changed_symbols =
            !files.changed_tests().is_empty() && !files.changes_openspec_change(change);
        let candidates =
            recovery_candidates(&surfaces, &code_delta, &evidence, recover_changed_symbols);
        let test_intent = files
            .changed_tests()
            .into_iter()
            .map(|test| TestIntentSummary {
                appears_to_expect: format!(
                    "the assertions in `{test}` remain valid on both revisions"
                ),
                test,
                changed_with_implementation: true,
            })
            .collect();
        let mut desk = RecoveryDesk::new(change);
        desk.recover(RecoveryInput {
            narrative,
            clusters,
            surface_delta: surfaces.clone(),
            test_intent,
            candidates,
            context: VerifyContext {
                existing_requirements,
                removed_endpoints: surfaces.removed,
                observed: Vec::new(),
            },
        });
        Ok(desk)
    }

    /// Replay measured coverage on base and head and build the live protection
    /// continuity view used by MCP and Studio.
    ///
    /// # Errors
    ///
    /// Missing revision-bound coverage, a failed runner, or incomplete graph
    /// evidence is refused rather than converted into an unprotected result.
    pub fn protection_view(
        &self,
        change: &str,
        base: &str,
        head: &str,
    ) -> Result<ProtectionView, BusError> {
        let compiled = self.compiled(change)?;
        let head_oracle = oracle_identity(&self.repo, &compiled)?;
        let range = self.revision_range(base, head)?;
        let files = changed_files(&self.repo, &range)?;
        let all_files = files.all();
        if all_files.is_empty() {
            return Err(BusError::Intelligence(format!(
                "revision range `{base}` -> `{head}` has no files to measure"
            )));
        }
        let revision = self.revision()?;
        let diff = self.weavatrix_operation(
            &revision,
            "graph_diff",
            &json!({
                "base_ref": range.merge_base,
                "detail": "edges",
                "max_results": 100_000
            }),
        )?;
        ensure_complete_diff(&diff)?;
        let head_graph = protection_graph_for_files(&self.repo, &revision, &all_files)?;
        let head_run = self.run(&RunCommand {
            change: compiled.change.clone(),
            base: range.base_ref.clone(),
            head: range.head_ref.clone(),
            scope: "all".into(),
            evidence_policy: "standard".into(),
        })?;
        if head_run.outcome != "passed" {
            return Err(BusError::Runtime(format!(
                "head protection run {} did not pass ({})",
                head_run.run_id, head_run.outcome
            )));
        }
        let head_snapshot = self.stored_protection_snapshot(&head_run.run_id)?;
        let (base_snapshot, base_graph, base_compiled, base_oracle) =
            self.measure_base_protection(&range, &all_files, &compiled.change)?;
        // Replaying the base suite is the one expensive part of protection, so
        // the measurement is persisted against the head run. `quality_verify`
        // then composes a real protection axis from stored evidence without
        // executing anything itself.
        self.persist_base_protection(&head_run.run_id, &base_snapshot)?;
        let oracle_replacement =
            if base_oracle.id == head_oracle.id && base_oracle.digest == head_oracle.digest {
                None
            } else {
                let (changed_obligations, obligation_replacements) =
                    expectation_change(&base_compiled.obligations, &compiled.obligations, true);
                let document = OracleReplacementDocument {
                    schema_v: 1,
                    change: compiled.change.clone(),
                    base_revision: range.merge_base.clone(),
                    head_revision: range.head_commit.clone(),
                    head_content_revision: revision.to_string(),
                    merge_base: range.merge_base.clone(),
                    base_seal: base_oracle.id,
                    base_seal_digest: base_oracle.digest,
                    head_seal: head_oracle.id,
                    head_seal_digest: head_oracle.digest,
                    changed_obligations,
                    obligation_replacements,
                };
                Some(self.persist_oracle_replacement(&head_run.run_id, &document)?)
            };
        Ok(build_protection_view(
            &compiled.obligations,
            &diff,
            (&base_snapshot, &head_snapshot),
            (&base_graph, &head_graph),
            &files,
            oracle_replacement,
        ))
    }

    /// Replay the browser programs on both revisions and ratchet the UI.
    ///
    /// Head evidence comes from a normal run, which already collects it. Base
    /// evidence needs the same programs against the merge-base, so this creates
    /// a temporary worktree and replays them there — the same shape as
    /// `protection_view`, and for the same reason: a regression is only
    /// meaningful against what the code used to do.
    ///
    /// The resulting delta is persisted against the head run, so a later
    /// `quality_verify` composes the UI axis from stored evidence without
    /// executing anything.
    ///
    /// # Errors
    ///
    /// A disabled policy, a head run that did not pass, missing base programs,
    /// or a malformed snapshot. Uses zero runtime model tokens.
    pub fn ui_integrity_view(
        &self,
        change: &str,
        base: &str,
        head: &str,
    ) -> Result<UiIntegrityDelta, BusError> {
        let compiled = self.compiled(change)?;
        let policy = load_ui_integrity_policy(&self.repo)?;
        if !policy.enabled {
            return Err(BusError::Runtime(
                "ui_integrity is not enabled in .weavatrix-quality/config.yaml".into(),
            ));
        }
        let range = self.revision_range(base, head)?;
        let head_run = self.run(&RunCommand {
            change: compiled.change.clone(),
            base: range.base_ref.clone(),
            head: range.head_ref.clone(),
            scope: "all".into(),
            evidence_policy: "standard".into(),
        })?;
        let store = self.store()?;
        let run_id =
            RunId::new(&head_run.run_id).map_err(|err| BusError::Identity(err.to_string()))?;
        let head_snapshot = Self::stored_ui_snapshot(&store, &run_id)?.ok_or_else(|| {
            BusError::Runtime(format!(
                "run {} collected no UI evidence; check that a browser program is selected",
                head_run.run_id
            ))
        })?;
        let base_snapshot = self.measure_base_ui(&range, &compiled, &policy)?;
        let previously_fixed = store
            .previously_fixed_debt()
            .map_err(|err| BusError::Store(err.to_string()))?
            .into_iter()
            .filter(|item| item.starts_with("ui:"))
            .collect::<BTreeSet<_>>();
        let mut delta = ratchet_ui(&base_snapshot, &head_snapshot, &previously_fixed, &policy);
        if policy.responsive.enabled {
            let (intervals, truncated) = self.measure_responsive_ui(
                &range,
                &compiled,
                &policy,
                &base_snapshot,
                &head_snapshot,
                &previously_fixed,
            )?;
            delta.responsive_intervals = intervals;
            delta.responsive_truncated = truncated;
        }
        // Remember what this change fixed so a later reintroduction is
        // `returned` rather than `new`.
        let fixed = delta.fixed_fingerprints();
        if !fixed.is_empty() {
            let revision = RevisionId::new(&head_snapshot.revision)
                .map_err(|err| BusError::Identity(err.to_string()))?;
            store
                .remember_fixed_debt(&fixed, &revision)
                .map_err(|err| BusError::Store(err.to_string()))?;
        }
        Self::persist_ui_delta(&store, &run_id, &base_snapshot, &delta)?;
        Ok(delta)
    }

    /// Read back the `ui-integrity-findings` artifact a run persisted.
    fn stored_ui_snapshot(
        store: &Store,
        run: &RunId,
    ) -> Result<Option<UiIntegritySnapshot>, BusError> {
        let Ok(document) = read_single_run_json(store, run, "ui-integrity-findings") else {
            return Ok(None);
        };
        if document.get("schema_v").and_then(Value::as_u64) != Some(1) {
            return Err(BusError::Store(
                "unknown ui-integrity-findings schema version".into(),
            ));
        }
        let findings: Vec<UiIntegrityFinding> =
            serde_json::from_value(document.get("findings").cloned().unwrap_or(json!([])))
                .map_err(|err| {
                    BusError::Store(format!("malformed stored ui-integrity findings: {err}"))
                })?;
        let measured_states = serde_json::from_value(
            document
                .get("measured_states")
                .cloned()
                .unwrap_or(json!([])),
        )
        .map_err(|err| BusError::Store(format!("malformed stored ui measured states: {err}")))?;
        Ok(Some(UiIntegritySnapshot {
            revision: document
                .get("revision")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            measured_states,
            findings,
            responsive_breakpoints: serde_json::from_value(
                document
                    .get("responsive_breakpoints")
                    .cloned()
                    .unwrap_or(json!([])),
            )
            .map_err(|err| {
                BusError::Store(format!("malformed stored responsive breakpoints: {err}"))
            })?,
            responsive_breakpoints_incomplete: document
                .get("responsive_breakpoints_incomplete")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            truncated: document
                .get("truncated")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        }))
    }

    /// Replay the exact head-selected programs against the merge-base runtime.
    ///
    /// The runtime coordinates (notably `base_url`) come from the base config,
    /// while program steps, seed, evidence policy, and sealed oracles are the
    /// exact head values already executed. This prevents a changed test from
    /// making the two sides incomparable and avoids treating preview origins
    /// as product behavior.
    fn replay_base_browser_programs(
        &self,
        range: &RevisionRange,
        head_policy: &BrowserPolicy,
        head_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
        ui_policy: &UiIntegrityPolicy,
        run_evidence_policy: &str,
    ) -> Result<BaseBrowserReplay, BusError> {
        let worktree = TemporaryWorktree::create(&self.repo, &range.merge_base)?;
        let revision = WeavatrixProvider
            .analyze(&worktree.path)
            .map_err(|err| BusError::Intelligence(err.to_string()))?
            .revision;
        let base_runtime =
            load_browser_runtime_with(&worktree.path, Some(head_policy.module_root.as_path()))?
                .ok_or_else(|| {
                    BusError::Runtime("merge base has no browser runtime configuration".into())
                })?;
        let cancel = Arc::new(AtomicBool::new(false));
        let mut runs = Vec::new();
        for (configured, _) in head_runs {
            let mut executable = configured.program.clone();
            cap_browser_evidence(&mut executable, run_evidence_policy);
            // Binary capture is not an input to structured comparison and its
            // timestamped paths are not behavior. The paired replay keeps the
            // exact actions, seed, oracles, network, console, and storage
            // policy while avoiding orphaned base-worktree files.
            executable.evidence_policy.screenshot = CaptureWhen::Never;
            executable.evidence_policy.trace = CaptureWhen::Never;
            let result = run_browser_program_at(
                &BrowserRunConfig {
                    base_url: base_runtime.base_url.clone(),
                    browser: base_runtime.browser.clone(),
                    headless: base_runtime.headless,
                    timeout: base_runtime.timeout,
                    module_root: base_runtime.module_root.clone(),
                    runtime_dir: self
                        .repo
                        .join(".weavatrix-quality/runtime/playwright-runner"),
                    evidence_dir: worktree
                        .path
                        .join(".weavatrix-quality/browser-evidence")
                        .join(format!(
                            "delta-base-{}",
                            safe_file_token(configured.program.id.as_str())
                        )),
                    viewport: None,
                    ui_integrity: ui_collection_config(ui_policy, &configured.oracles),
                    cancel: Arc::clone(&cancel),
                },
                &executable,
                &configured.oracles,
                revision.as_str(),
            )
            .map_err(|err| BusError::Runtime(err.to_string()))?;
            runs.push(result);
        }
        Ok(BaseBrowserReplay { revision, runs })
    }

    /// Replay the configured browser programs at the merge base.
    fn measure_base_ui(
        &self,
        range: &RevisionRange,
        compiled: &Compiled,
        policy: &UiIntegrityPolicy,
    ) -> Result<UiIntegritySnapshot, BusError> {
        // The browser engine is toolchain, not source: a fresh worktree has no
        // node_modules, and replaying base with a different engine would
        // confound the geometry being compared.
        let engine = load_browser_policy(&self.repo, &compiled.obligations)?
            .map(|policy| policy.module_root)
            .ok_or_else(|| {
                BusError::Runtime("no browser runtime is configured for this repository".into())
            })?;
        let worktree = TemporaryWorktree::create(&self.repo, &range.merge_base)?;
        let evidence = WeavatrixProvider
            .analyze(&worktree.path)
            .map_err(|err| BusError::Intelligence(err.to_string()))?;
        let Some(browser) =
            load_browser_policy_with(&worktree.path, &compiled.obligations, Some(&engine))?
        else {
            // Base had no browser programs at all, so nothing here can be
            // compared. Report it rather than calling head's findings new.
            return Ok(UiIntegritySnapshot {
                revision: evidence.revision.to_string(),
                truncated: true,
                ..UiIntegritySnapshot::default()
            });
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let mut runs = Vec::new();
        for configured in &browser.programs {
            let result = run_browser_program_at(
                &BrowserRunConfig {
                    base_url: browser.base_url.clone(),
                    browser: browser.browser.clone(),
                    headless: browser.headless,
                    timeout: browser.timeout,
                    module_root: browser.module_root.clone(),
                    // The bridge is materialized once next to the engine.
                    runtime_dir: self
                        .repo
                        .join(".weavatrix-quality/runtime/playwright-runner"),
                    evidence_dir: worktree
                        .path
                        .join(".weavatrix-quality")
                        .join("browser-evidence")
                        .join(safe_file_token(configured.program.id.as_str())),
                    viewport: None,
                    ui_integrity: ui_collection_config(policy, &configured.oracles),
                    cancel: Arc::clone(&cancel),
                },
                &configured.program,
                &configured.oracles,
                evidence.revision.as_str(),
            )
            .map_err(|err| BusError::Runtime(err.to_string()))?;
            runs.push((configured, result));
        }
        let borrowed = runs
            .iter()
            .map(|(configured, result)| (*configured, result.clone()))
            .collect::<Vec<_>>();
        Ok(analyse_ui_snapshots(&evidence.revision, policy, &borrowed)?.snapshot)
    }

    /// Probe the parsed CSS/container boundaries on base and head, then bisect
    /// only intervals whose measured finding sets disagree.
    #[allow(clippy::too_many_arguments)]
    fn measure_responsive_ui(
        &self,
        range: &RevisionRange,
        compiled: &Compiled,
        policy: &UiIntegrityPolicy,
        base_default: &UiIntegritySnapshot,
        head_default: &UiIntegritySnapshot,
        previously_fixed: &BTreeSet<String>,
    ) -> Result<(Vec<wvq_ui::ResponsiveFailureInterval>, bool), BusError> {
        let engine = load_browser_policy(&self.repo, &compiled.obligations)?
            .map(|browser| browser.module_root)
            .ok_or_else(|| {
                BusError::Runtime("no browser runtime is configured for this repository".into())
            })?;
        let base_worktree = TemporaryWorktree::create(&self.repo, &range.merge_base)?;
        let base_revision = WeavatrixProvider
            .analyze(&base_worktree.path)
            .map_err(|err| BusError::Intelligence(err.to_string()))?
            .revision;
        let base_browser =
            load_browser_policy_with(&base_worktree.path, &compiled.obligations, Some(&engine))?
                .ok_or_else(|| {
                    BusError::Runtime("base has no browser runtime configuration".into())
                })?;
        let head_browser =
            load_browser_policy_with(&self.repo, &compiled.obligations, Some(&engine))?
                .ok_or_else(|| {
                    BusError::Runtime("head has no browser runtime configuration".into())
                })?;
        let head_revision = RevisionId::new(&head_default.revision)
            .map_err(|err| BusError::Identity(err.to_string()))?;

        let breakpoints = base_default
            .responsive_breakpoints
            .union(&head_default.responsive_breakpoints)
            .copied()
            .collect::<BTreeSet<_>>();
        let plan = responsive_probe_plan(&policy.responsive, &breakpoints);
        let mut truncated = plan.truncated
            || base_default.responsive_breakpoints_incomplete
            || head_default.responsive_breakpoints_incomplete;
        let mut probes = Vec::new();
        for width in plan.widths {
            probes.push(self.measure_responsive_probe_with_retry(
                width,
                &base_worktree.path,
                &base_revision,
                &base_browser,
                &head_revision,
                &head_browser,
                policy,
                previously_fixed,
            )?);
        }
        while let Some(width) = next_responsive_probe(&policy.responsive, &probes) {
            probes.push(self.measure_responsive_probe_with_retry(
                width,
                &base_worktree.path,
                &base_revision,
                &base_browser,
                &head_revision,
                &head_browser,
                policy,
                previously_fixed,
            )?);
        }
        probes.sort_by_key(|probe| probe.width);
        truncated |= probes
            .iter()
            .any(|probe| probe.delta.truncated || !probe.delta.unmeasured_states.is_empty());
        let exhaustive_policy = wvq_ui::ResponsivePolicy {
            max_probes: 128,
            ..policy.responsive
        };
        truncated |= next_responsive_probe(&exhaustive_policy, &probes).is_some();
        Ok((
            responsive_failure_intervals(&policy.responsive, &probes),
            truncated,
        ))
    }

    /// A browser can report a bounded transient collection limitation (for
    /// example, one state still settling) even when the repository is static.
    /// Retry that exact width once. A second incomplete measurement is kept and
    /// therefore still fails closed; retries never turn missing evidence into a
    /// pass.
    #[allow(clippy::too_many_arguments)]
    fn measure_responsive_probe_with_retry(
        &self,
        width: u32,
        base_repo: &Path,
        base_revision: &RevisionId,
        base_browser: &BrowserPolicy,
        head_revision: &RevisionId,
        head_browser: &BrowserPolicy,
        policy: &UiIntegrityPolicy,
        previously_fixed: &BTreeSet<String>,
    ) -> Result<ResponsiveProbe, BusError> {
        let first = self.measure_responsive_probe(
            width,
            base_repo,
            base_revision,
            base_browser,
            head_revision,
            head_browser,
            policy,
            previously_fixed,
        )?;
        if !responsive_probe_incomplete(&first) {
            return Ok(first);
        }
        self.measure_responsive_probe(
            width,
            base_repo,
            base_revision,
            base_browser,
            head_revision,
            head_browser,
            policy,
            previously_fixed,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn measure_responsive_probe(
        &self,
        width: u32,
        base_repo: &Path,
        base_revision: &RevisionId,
        base_browser: &BrowserPolicy,
        head_revision: &RevisionId,
        head_browser: &BrowserPolicy,
        policy: &UiIntegrityPolicy,
        previously_fixed: &BTreeSet<String>,
    ) -> Result<ResponsiveProbe, BusError> {
        let viewport = BrowserViewport {
            width,
            height: policy.responsive.height,
        };
        let base = self.measure_ui_at(
            base_repo,
            base_revision,
            base_browser,
            policy,
            viewport,
            "base",
        )?;
        let head = self.measure_ui_at(
            &self.repo,
            head_revision,
            head_browser,
            policy,
            viewport,
            "head",
        )?;
        Ok(ResponsiveProbe {
            width,
            delta: ratchet_ui(&base, &head, previously_fixed, policy),
        })
    }

    fn measure_ui_at(
        &self,
        repo: &Path,
        revision: &RevisionId,
        browser: &BrowserPolicy,
        policy: &UiIntegrityPolicy,
        viewport: BrowserViewport,
        side: &str,
    ) -> Result<UiIntegritySnapshot, BusError> {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut runs = Vec::new();
        for configured in &browser.programs {
            let result = run_browser_program_at(
                &BrowserRunConfig {
                    base_url: browser.base_url.clone(),
                    browser: browser.browser.clone(),
                    headless: browser.headless,
                    timeout: browser.timeout,
                    module_root: browser.module_root.clone(),
                    runtime_dir: self
                        .repo
                        .join(".weavatrix-quality/runtime/playwright-runner"),
                    evidence_dir: repo
                        .join(".weavatrix-quality")
                        .join("browser-evidence")
                        .join(format!(
                            "responsive-{side}-{}-{}",
                            viewport.width,
                            safe_file_token(configured.program.id.as_str())
                        )),
                    viewport: Some(viewport),
                    ui_integrity: ui_collection_config(policy, &configured.oracles),
                    cancel: Arc::clone(&cancel),
                },
                &configured.program,
                &configured.oracles,
                revision.as_str(),
            )
            .map_err(|err| BusError::Runtime(err.to_string()))?;
            runs.push((configured, result));
        }
        let borrowed = runs
            .iter()
            .map(|(configured, result)| (*configured, result.clone()))
            .collect::<Vec<_>>();
        Ok(analyse_ui_snapshots(revision, policy, &borrowed)?.snapshot)
    }

    /// Store the base snapshot and the classified delta on the head run.
    fn persist_ui_delta(
        store: &Store,
        run: &RunId,
        base: &UiIntegritySnapshot,
        delta: &UiIntegrityDelta,
    ) -> Result<(), BusError> {
        let mut handles = Vec::new();
        if read_single_run_json(store, run, "base-ui-integrity-findings").is_err() {
            put_bounded_ui_artifact(
                store,
                run,
                &format!("artifact-{}-base-ui-integrity", run.as_str()),
                "base-ui-integrity-findings",
                &json!({
                    "schema_v": 1,
                    "revision": base.revision,
                    "measured_states": base.measured_states,
                    "findings": base.findings,
                    "responsive_breakpoints": base.responsive_breakpoints,
                    "responsive_breakpoints_incomplete": base.responsive_breakpoints_incomplete,
                    "truncated": base.truncated,
                }),
                &mut handles,
            )?;
        }
        if read_single_run_json(store, run, UI_INTEGRITY_DELTA_KIND).is_ok() {
            return Ok(());
        }
        put_bounded_ui_artifact(
            store,
            run,
            &format!("artifact-{}-ui-integrity-delta", run.as_str()),
            UI_INTEGRITY_DELTA_KIND,
            &ui_delta_document(delta),
            &mut handles,
        )
    }

    /// Attach the measured base snapshot to the head run, idempotently.
    fn persist_base_protection(
        &self,
        run: &str,
        base: &ProtectionSnapshot,
    ) -> Result<(), BusError> {
        let run = RunId::new(run).map_err(|err| BusError::Identity(err.to_string()))?;
        let store = self.store()?;
        if snapshot_artifact(&store, &run, "base-protection-snapshot")?.is_some() {
            return Ok(());
        }
        let mut handles = Vec::new();
        put_json_run_artifact(
            &store,
            &run,
            &format!("{run}-base-protection-snapshot"),
            "base-protection-snapshot",
            base,
            &mut handles,
        )
    }

    /// Persist one immutable, revision-bound expectation replacement proposal.
    ///
    /// A human decision is stored separately and must match both the derived
    /// subject and the CAS digest of these exact bytes.
    fn persist_oracle_replacement(
        &self,
        run: &str,
        document: &OracleReplacementDocument,
    ) -> Result<OracleReplacementReview, BusError> {
        let run = RunId::new(run).map_err(|err| BusError::Identity(err.to_string()))?;
        let store = self.store()?;
        if let Some((stored, review)) = stored_oracle_replacement(&store, &run)? {
            if &stored != document {
                return Err(BusError::Ambiguous(format!(
                    "run {run} already carries a different OracleSeal replacement proposal"
                )));
            }
            return Ok(review);
        }
        let bytes = serde_json::to_vec_pretty(document).map_err(|err| {
            BusError::Runtime(format!("cannot encode OracleSeal replacement: {err}"))
        })?;
        let mut handles = Vec::new();
        put_run_artifact(
            &store,
            &run,
            &format!("artifact-{run}-oracle-replacement"),
            ORACLE_REPLACEMENT_KIND,
            &bytes,
            &mut handles,
        )?;
        stored_oracle_replacement(&store, &run)?
            .map(|(_, review)| review)
            .ok_or_else(|| {
                BusError::Store(format!(
                    "run {run} did not retain its OracleSeal replacement proposal"
                ))
            })
    }

    /// Gather every axis `quality_verify` composes, from stored evidence only.
    ///
    /// `verify` never executes. Each axis therefore reports one of three honest
    /// states: measured, `not_applicable` when this change has no surface the
    /// axis can see, or `unmeasured` when it does have that surface and the
    /// evidence is absent. No axis is silently reported as clean.
    fn verdict_inputs(
        &self,
        compiled: &Compiled,
        run: Option<&StoredRun>,
        proofs: Vec<ProofOutcome>,
    ) -> Result<VerdictInputs, BusError> {
        let Some(run) = run else {
            // Nothing ran at this revision. The proof axis still reports the
            // gap; the execution-backed axes have no surface to measure.
            return Ok(VerdictInputs {
                proofs,
                ..VerdictInputs::default()
            });
        };
        let store = self.store()?;
        let range = stored_range(&store, &run.id);
        let (protection, protection_limits) = self.protection_axis(&store, run, compiled)?;
        let (debt, debt_limits) = self.debt_axis(compiled, range.as_ref());
        let (stability, stability_limits) = stability_axis(&self.repo, &store, run, compiled);
        let ai = self.ai_axis(&store, compiled)?;
        let (ui_integrity, ui_limits) = ui_integrity_axis(&store, run, compiled)?;
        let (delta_triangle, delta_limits) = delta_triangle_axis(&store, run)?;
        let mut limitations = protection_limits;
        limitations.extend(debt_limits);
        limitations.extend(stability_limits);
        limitations.extend(ui_limits);
        limitations.extend(delta_limits);
        Ok(VerdictInputs {
            proofs,
            protection,
            debt,
            stability,
            ai,
            ui_integrity,
            delta_triangle,
            limitations,
        })
    }

    /// Protection continuity from the two stored snapshots.
    ///
    /// The head snapshot is written by every run that produced measured
    /// coverage. The base snapshot is written by `protection_view`, which is the
    /// only path allowed to replay the base suite; `verify` reuses it instead of
    /// re-running anything. With head coverage but no base snapshot the axis is
    /// `unmeasured`, never `clean` — a change cannot be shown to have preserved
    /// protection it never compared against.
    fn protection_axis(
        &self,
        store: &Store,
        run: &StoredRun,
        compiled: &Compiled,
    ) -> Result<(ProtectionAxis, Vec<Limitation>), BusError> {
        let head = snapshot_artifact(store, &run.id, "protection-snapshot")?;
        let base = snapshot_artifact(store, &run.id, "base-protection-snapshot")?;
        match (base, head) {
            (Some(base), Some(head)) => {
                let oracle_replacement = stored_oracle_replacement(store, &run.id)?;
                let review = oracle_replacement.as_ref().map(|(_, review)| review);
                if let Some((document, _)) = &oracle_replacement {
                    let range = stored_range(store, &run.id).ok_or_else(|| {
                        BusError::Store(format!(
                            "run {} has an OracleSeal replacement without revision-range evidence",
                            run.id
                        ))
                    })?;
                    let current_oracle = oracle_identity(&self.repo, compiled)?;
                    if document.change != compiled.change
                        || document.base_revision != range.merge_base
                        || document.head_revision != range.head_commit
                        || document.head_content_revision != range.head_content_revision
                        || document.merge_base != range.merge_base
                        || document.head_content_revision != run.revision.as_str()
                        || document.head_seal != current_oracle.id
                        || document.head_seal_digest != current_oracle.digest
                    {
                        return Err(BusError::Ambiguous(format!(
                            "run {} carries an OracleSeal replacement for a different change, revision range, or head seal",
                            run.id
                        )));
                    }
                }
                let context = DeltaContext {
                    relocations: snapshot_relocations(&base, &head),
                    changed_obligations: review
                        .map(|review| review.changed_obligations.clone())
                        .unwrap_or_default(),
                    obligation_replacements: review
                        .map(|review| review.obligation_replacements.clone())
                        .unwrap_or_default(),
                    oracle_replacement_approved: review.is_some_and(|review| review.approved),
                    approved_replaced_flows: approved_replaced_flows(&base, &head, review),
                    ..DeltaContext::default()
                };
                let deltas = protection_delta(&base, &head, &context);
                let any_high_risk = compiled
                    .obligations
                    .iter()
                    .any(|item| matches!(item.risk, RiskLevel::High | RiskLevel::Critical));
                let high_risk_flows = if any_high_risk {
                    deltas.iter().map(|item| item.flow.clone()).collect()
                } else {
                    Vec::new()
                };
                let findings = gate_protection(&ProtectionCheckInput {
                    deltas: deltas.clone(),
                    tests: protection_test_changes(
                        &base,
                        &head,
                        &deltas,
                        &BTreeSet::new(),
                        &context,
                    ),
                    trends: Vec::new(),
                    policy: ProtectionPolicy {
                        high_risk_flows,
                        substitution_ratio: 10,
                    },
                });
                Ok((protection_axis_from(&deltas, &findings), Vec::new()))
            }
            (_, Some(_)) => Ok((
                ProtectionAxis {
                    state: AxisState::Unmeasured,
                    ..ProtectionAxis::default()
                },
                vec![Limitation {
                    axis: "protection".into(),
                    detail: "head protection was measured but no base snapshot exists; \
                             run the protection profile to replay the base suite"
                        .into(),
                }],
            )),
            // No coverage reached the impacted graph at all: this change has no
            // protection surface to compare.
            (_, None) => Ok((ProtectionAxis::default(), Vec::new())),
        }
    }

    /// Debt ratchet over the exact range the run measured.
    ///
    /// Weavatrix `run_audit` is a read-only graph query, so it is safe on the
    /// verify path. Any failure degrades to `unmeasured` rather than turning a
    /// missing comparison into a clean axis or aborting the whole verdict.
    fn debt_axis(
        &self,
        compiled: &Compiled,
        range: Option<&RevisionRange>,
    ) -> (DebtAxis, Vec<Limitation>) {
        let Some(range) = range else {
            return (DebtAxis::default(), Vec::new());
        };
        match self.debt(&DebtCommand {
            change: compiled.change.clone(),
            base: range.base_ref.clone(),
            head: range.head_ref.clone(),
        }) {
            Ok(reply) => (debt_axis_from(&reply), Vec::new()),
            Err(err) => (
                DebtAxis {
                    state: AxisState::Unmeasured,
                    ..DebtAxis::default()
                },
                vec![Limitation {
                    axis: "debt".into(),
                    detail: format!("base/head debt comparison is unavailable: {err}"),
                }],
            ),
        }
    }

    /// Measured AI spend for this change. The ordinary green path spends none.
    fn ai_axis(&self, store: &Store, compiled: &Compiled) -> Result<AiAxis, BusError> {
        let persisted = store
            .ai_usage_for_change(&compiled.change)
            .map_err(|err| BusError::Store(err.to_string()))?
            .unwrap_or_default();
        if persisted.planning_tokens == 0
            && persisted.runtime_tokens == 0
            && persisted.browser_escape_calls == 0
            && persisted.vision_calls == 0
        {
            // Nothing was ever charged to this change: the axis has no surface.
            return Ok(AiAxis::default());
        }
        let usage = AiUsage {
            planning_tokens: persisted.planning_tokens,
            runtime_tokens: persisted.runtime_tokens,
            browser_escape_calls: u32::try_from(persisted.browser_escape_calls).unwrap_or(u32::MAX),
            vision_calls: u32::try_from(persisted.vision_calls).unwrap_or(u32::MAX),
            cost_micros: persisted.cost_micros,
        };
        let budget_exhausted = load_model_policy(&self.repo)
            .is_ok_and(|policy| AiCostFirewall::with_usage(policy.budget, usage).is_exhausted());
        Ok(AiAxis {
            state: if budget_exhausted {
                AxisState::Warnings
            } else {
                AxisState::Clean
            },
            runtime_tokens: persisted.runtime_tokens,
            budget_exhausted,
            // A decision only becomes an unresolved blocker once a caller
            // records one. WVQ never invents a pending decision.
            unresolved_decisions: Vec::new(),
        })
    }

    fn stored_protection_snapshot(&self, run: &str) -> Result<ProtectionSnapshot, BusError> {
        let run = RunId::new(run).map_err(|err| BusError::Identity(err.to_string()))?;
        let store = self.store()?;
        let mut found = None;
        for artifact in store
            .run_artifacts(&run)
            .map_err(|err| BusError::Store(err.to_string()))?
        {
            let (record, bytes) = store
                .read_artifact(&artifact)
                .map_err(|err| BusError::Store(err.to_string()))?;
            if record.kind != "protection-snapshot" {
                continue;
            }
            if found.is_some() {
                return Err(BusError::Store(format!(
                    "run {run} has more than one protection snapshot"
                )));
            }
            found = Some(serde_json::from_slice(&bytes).map_err(|err| {
                BusError::Store(format!("invalid protection snapshot on run {run}: {err}"))
            })?);
        }
        found.ok_or_else(|| {
            BusError::Runtime(format!(
                "run {run} produced no measured protection snapshot; coverage is required"
            ))
        })
    }

    fn measure_base_protection(
        &self,
        range: &RevisionRange,
        files: &[String],
        change: &str,
    ) -> Result<(ProtectionSnapshot, Value, Compiled, OracleIdentity), BusError> {
        if let Some(err) = &self.executor_init_error {
            return Err(BusError::Runtime(format!(
                "registered executor initialization failed: {err}"
            )));
        }
        let worktree = TemporaryWorktree::create(&self.repo, &range.merge_base)?;
        let compiled = compile_repository(&worktree.path, change)?;
        let oracle = oracle_identity(&worktree.path, &compiled)?;
        let evidence = WeavatrixProvider
            .analyze(&worktree.path)
            .map_err(|err| BusError::Intelligence(err.to_string()))?;
        let graph = protection_graph_for_files(&worktree.path, &evidence.revision, files)?;
        let targets = discover_executor_targets(&worktree.path)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        if targets.is_empty() {
            return Err(BusError::Runtime(
                "base revision has no supported registered executor".into(),
            ));
        }
        let records = execute_full_targets(
            &self.executors,
            &worktree.path,
            &targets,
            &Arc::new(AtomicBool::new(false)),
        )?;
        if records
            .iter()
            .any(|record| !record.passed || record.error.is_some())
        {
            return Err(BusError::Runtime(
                "base protection replay did not pass every registered runner".into(),
            ));
        }
        let bindings = load_test_bindings(&worktree.path)?;
        let protection = live_protection_snapshot(
            &worktree.path,
            &evidence.revision,
            &graph,
            &records,
            &bindings,
        )?
        .ok_or_else(|| {
            BusError::Runtime(
                "base protection replay produced no coverage for the impacted graph".into(),
            )
        })?;
        Ok((protection, graph, compiled, oracle))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<RunState>> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn compiled(&self, change: &str) -> Result<Compiled, BusError> {
        compile_repository(&self.repo, change)
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
        let merge_base = self.resolve_merge_base(&base_commit, &head_commit)?;
        let head_content_revision = self.revision()?.to_string();
        Ok(RevisionRange {
            base_ref: base.to_owned(),
            base_commit,
            head_ref: head.to_owned(),
            head_commit,
            head_content_revision,
            merge_base,
        })
    }

    fn resolve_merge_base(&self, base: &str, head: &str) -> Result<String, BusError> {
        let output = ProcessCommand::new("git")
            .args(["merge-base", "--", base, head])
            .current_dir(&self.repo)
            .output()
            .map_err(|err| {
                BusError::Intelligence(format!("cannot resolve Git merge-base: {err}"))
            })?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(BusError::Intelligence(format!(
                "cannot resolve a common ancestor for `{base}` and `{head}`{}",
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(": {detail}")
                }
            )));
        }
        let commit = String::from_utf8(output.stdout).map_err(|err| {
            BusError::Intelligence(format!("Git returned a non-UTF-8 merge-base: {err}"))
        })?;
        let commit = commit.trim();
        if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(BusError::Intelligence(format!(
                "Git returned an invalid merge-base `{commit}`"
            )));
        }
        Ok(commit.to_owned())
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct OracleIdentity {
    id: String,
    digest: String,
}

/// Immutable proposal bytes stored in CAS before a human decision exists.
/// Approval state is deliberately not part of this document or its digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct OracleReplacementDocument {
    schema_v: u32,
    change: String,
    base_revision: String,
    head_revision: String,
    head_content_revision: String,
    merge_base: String,
    base_seal: String,
    base_seal_digest: String,
    head_seal: String,
    head_seal_digest: String,
    changed_obligations: Vec<String>,
    obligation_replacements: Vec<(String, String)>,
}

fn compile_repository(repo: &Path, change: &str) -> Result<Compiled, BusError> {
    let change = resolve_change(repo, change)?;
    let spec = read_change(repo, &change)?;
    let contract = load_quality_contract(repo, &change)?;
    let obligations = compile_obligations(&contract, &spec)?;
    Ok(Compiled {
        change,
        spec,
        obligations,
    })
}

fn oracle_identity(repo: &Path, compiled: &Compiled) -> Result<OracleIdentity, BusError> {
    let contract = load_quality_contract(repo, &compiled.change)?;
    let oracle = seal(&contract, &compiled.obligations, &compiled.spec)?;
    Ok(OracleIdentity {
        id: oracle.id.to_string(),
        digest: oracle.digest.to_string(),
    })
}

struct RevisionRange {
    base_ref: String,
    base_commit: String,
    head_ref: String,
    head_commit: String,
    head_content_revision: String,
    merge_base: String,
}

#[derive(Default)]
struct ChangedFiles {
    added: Vec<String>,
    changed: Vec<String>,
    removed: Vec<String>,
}

impl ChangedFiles {
    fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }

    fn tests_delta(&self) -> TestsDelta {
        TestsDelta {
            added: self
                .added
                .iter()
                .filter(|path| is_test_path(path))
                .cloned()
                .collect(),
            changed: self
                .changed
                .iter()
                .filter(|path| is_test_path(path))
                .cloned()
                .collect(),
            removed: self
                .removed
                .iter()
                .filter(|path| is_test_path(path))
                .cloned()
                .collect(),
        }
    }

    fn changed_tests(&self) -> Vec<String> {
        let mut tests = self.tests_delta();
        tests.added.append(&mut tests.changed);
        tests.added.append(&mut tests.removed);
        tests.added.sort();
        tests.added.dedup();
        tests.added
    }

    fn all(&self) -> Vec<String> {
        let mut files = self.added.clone();
        files.extend(self.changed.iter().cloned());
        files.extend(self.removed.iter().cloned());
        files.sort();
        files.dedup();
        files
    }

    fn changes_openspec_change(&self, change: &str) -> bool {
        let prefix = format!("openspec/changes/{change}/");
        self.all().iter().any(|path| path.starts_with(&prefix))
    }
}

struct TemporaryWorktree {
    repo: PathBuf,
    path: PathBuf,
}

impl TemporaryWorktree {
    fn create(repo: &Path, commit: &str) -> Result<Self, BusError> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| BusError::Identity(err.to_string()))?
            .as_nanos();
        let short = commit.get(..12).unwrap_or(commit);
        let path =
            std::env::temp_dir().join(format!("wvq-base-{}-{}-{nanos}", std::process::id(), short));
        if path.exists() {
            return Err(BusError::Runtime(format!(
                "temporary base worktree path already exists: {}",
                path.display()
            )));
        }
        git_output(
            repo,
            &[
                "worktree".into(),
                "add".into(),
                "--detach".into(),
                path.display().to_string(),
                commit.to_owned(),
            ],
        )?;
        Ok(Self {
            repo: repo.to_path_buf(),
            path,
        })
    }
}

impl Drop for TemporaryWorktree {
    fn drop(&mut self) {
        let _ = ProcessCommand::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&self.path)
            .current_dir(&self.repo)
            .output();
        let _ = ProcessCommand::new("git")
            .args(["worktree", "prune"])
            .current_dir(&self.repo)
            .output();
    }
}

#[derive(Debug)]
struct ExecutorRecord {
    executor: String,
    cwd: String,
    selection: Vec<String>,
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
    runner: Option<String>,
    suite: Option<String>,
    case: Option<String>,
    obligations: BTreeSet<String>,
    cost: u64,
    flake_penalty: u64,
}

struct BrowserPolicy {
    base_url: String,
    browser: String,
    headless: bool,
    timeout: Duration,
    module_root: PathBuf,
    programs: Vec<ConfiguredBrowserProgram>,
}

struct ConfiguredBrowserProgram {
    path: String,
    program: TestProgram,
    oracles: Vec<ProgramOracle>,
}

struct BaseBrowserReplay {
    revision: RevisionId,
    runs: Vec<BrowserProgramRun>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredBrowserProgramEvidence {
    schema_v: u32,
    program: String,
    asserted: Vec<String>,
    contradicted: Vec<String>,
    assertions: Vec<StoredBrowserAssertionEvidence>,
    present: Vec<EvidenceKind>,
    observations: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredBrowserAssertionEvidence {
    obligation: String,
    step: usize,
    status: String,
    observation: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredObligationExecutionMap {
    schema_v: u32,
    obligations: BTreeMap<String, Vec<StoredObligationExecution>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredObligationExecution {
    executor: String,
    path: String,
    suite: String,
    case: String,
    status: String,
    invocation_passed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    assertion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    observation: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRevisionRangeEvidence {
    schema_v: u32,
    base: StoredRevisionEndpoint,
    head: StoredRevisionEndpoint,
    merge_base: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRevisionEndpoint {
    #[serde(rename = "ref")]
    reference: String,
    commit: String,
    #[serde(default)]
    content_revision: Option<String>,
}

#[derive(Default)]
struct BrowserProofEvidence {
    programs: BTreeSet<String>,
    present: Vec<EvidenceKind>,
    observations: Vec<String>,
    passed: bool,
    failed: bool,
    contradicted: bool,
}

#[derive(Default)]
struct BehaviorContributionSummary {
    states: u64,
    new_states: u64,
    edges: u64,
    new_edges: u64,
}

struct ProgramBehaviorContribution {
    states: BTreeSet<String>,
    new_states: BTreeSet<String>,
    edges: BTreeSet<String>,
    new_edges: BTreeSet<String>,
    api_operations: BTreeSet<String>,
    artifact: Value,
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
    filters: Vec<String>,
    selected_tests: Vec<String>,
}

type FilterGroups = BTreeMap<(String, String), (ExecutorTarget, Vec<(String, String)>)>;

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
        let configured_browser = load_browser_policy(&self.repo, &compiled.obligations)?;
        if targets.is_empty()
            && configured_browser
                .as_ref()
                .is_none_or(|policy| policy.programs.is_empty())
        {
            let store = self.store()?;
            let promoted = store
                .latest_program_revisions_for_change(&compiled.change)
                .map_err(|err| BusError::Store(err.to_string()))?;
            if configured_browser.is_none() || promoted.is_empty() {
                return Err(BusError::Runtime(
                    "no supported registered executor or browser TestProgram was discovered".into(),
                ));
            }
        }
        let range = self.revision_range(&cmd.base, &cmd.head)?;
        let changed = changed_files(&self.repo, &range)?;
        let store = self.store()?;
        let browser = load_live_browser_policy(&self.repo, &compiled, &store)?;
        if targets.is_empty()
            && browser
                .as_ref()
                .is_none_or(|policy| policy.programs.is_empty())
        {
            return Err(BusError::Runtime(
                "no supported registered executor or browser TestProgram was discovered".into(),
            ));
        }
        for target in &targets {
            clear_generated_runner_artifacts(&target.cwd)?;
        }
        let before = self.revision()?;
        let protection_graph = protection_graph_for_files(&self.repo, &before, &changed.all())?;
        let graph_diff = self.weavatrix_operation(
            &before,
            "graph_diff",
            &json!({
                "base_ref": range.merge_base,
                "detail": "edges",
                "max_results": 100_000
            }),
        )?;
        let change_impact = self.weavatrix_operation(
            &before,
            "change_impact",
            &json!({
                "base_ref": range.merge_base,
                "depth": 6,
                "max_nodes": 100_000,
                "precision": "graph"
            }),
        )?;
        let static_selection = self.weavatrix_operation(
            &before,
            "select_tests",
            &json!({
                "base_ref": range.merge_base,
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
        let browser_bindings = browser
            .as_ref()
            .map_or_else(Vec::new, browser_test_bindings);
        let impact = live_impacted_surface(&graph_diff, &change_impact)?;
        let historical_selection = historical_selection_candidates(&store, &impact)?;
        let live_selection = build_live_selection(
            &self.repo,
            &static_selection,
            &graph_diff,
            &impact,
            &obligation_needs,
            &browser_bindings,
            &historical_selection,
        )?;
        let browser_paths = browser_bindings
            .iter()
            .map(|binding| binding.path.clone())
            .collect::<BTreeSet<_>>();
        let available_test_count =
            available_test_paths(&self.repo, &targets, &browser_paths)?.len();
        let (execution_requests, effective_scope, scope_reason, executed_tests) =
            build_execution_requests(
                &self.repo,
                &targets,
                &live_selection,
                &browser_paths,
                &cmd.scope,
            );
        let mut records = Vec::new();
        for request in &execution_requests {
            let target = &request.target;
            std::fs::create_dir_all(target.cwd.join(".weavatrix-quality")).map_err(|err| {
                BusError::Runtime(format!(
                    "cannot prepare runner evidence directory in {}: {err}",
                    target.cwd.display()
                ))
            })?;
            clear_generated_runner_artifacts(&target.cwd)?;
            let prepared = self
                .executors
                .prepare(PrepareRequest {
                    executor: target.executor.clone(),
                    cwd: target.cwd.clone(),
                    filters: request.filters.clone(),
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
                    selection: request.selected_tests.clone(),
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
                    selection: request.selected_tests.clone(),
                    status_code: None,
                    passed: false,
                    error: Some(err.to_string()),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    artifacts: Vec::new(),
                },
            };
            attach_normalized_artifacts(&self.repo, &target.cwd, started, &mut record);
            clear_generated_runner_artifacts(&target.cwd)?;
            records.push(record);
        }

        let ui_policy = load_ui_integrity_policy(&self.repo)?;
        let mut browser_runs = Vec::new();
        if let Some(policy) = &browser {
            for configured in policy.programs.iter().filter(|program| {
                executed_tests
                    .as_ref()
                    .is_none_or(|selected| selected.contains(&program.path))
            }) {
                let mut executable = configured.program.clone();
                cap_browser_evidence(&mut executable, &cmd.evidence_policy);
                let evidence_dir = self
                    .repo
                    .join(".weavatrix-quality")
                    .join("browser-evidence")
                    .join(safe_file_token(configured.program.id.as_str()));
                let result = run_browser_program_at(
                    &BrowserRunConfig {
                        base_url: policy.base_url.clone(),
                        browser: policy.browser.clone(),
                        headless: policy.headless,
                        timeout: policy.timeout,
                        module_root: policy.module_root.clone(),
                        runtime_dir: self
                            .repo
                            .join(".weavatrix-quality/runtime/playwright-runner"),
                        evidence_dir,
                        viewport: None,
                        ui_integrity: ui_collection_config(&ui_policy, &configured.oracles),
                        cancel: Arc::clone(&cancel),
                    },
                    &executable,
                    &configured.oracles,
                    before.as_str(),
                )
                .map_err(|err| BusError::Runtime(err.to_string()))?;
                browser_runs.push((configured, result));
            }
        }

        // Differential replay is part of the normal browser run, not an
        // opt-in reporting view. A failure to obtain the base side is retained
        // as unmeasured evidence after the head run itself is stored.
        let base_browser_replay = browser.as_ref().and_then(|policy| {
            (!browser_runs.is_empty()).then(|| {
                self.replay_base_browser_programs(
                    &range,
                    policy,
                    &browser_runs,
                    &ui_policy,
                    &cmd.evidence_policy,
                )
            })
        });

        let after = self.revision()?;
        if before != after {
            return Err(BusError::Ambiguous(format!(
                "repository revision changed during execution: `{before}` -> `{after}`"
            )));
        }
        let outcome = if records.iter().any(|record| record.error.is_some()) {
            "error"
        } else if records.iter().all(|record| record.passed)
            && browser_runs.iter().all(|(_, run)| run.passed)
        {
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
                "schema_v": 2,
                "base": {"ref": range.base_ref, "commit": range.base_commit},
                "head": {
                    "ref": range.head_ref,
                    "commit": range.head_commit,
                    "content_revision": before.as_str()
                },
                "merge_base": range.merge_base
            }),
            &mut handles,
        )?;
        put_json_run_artifact(
            &store,
            &run_id,
            &format!("artifact-{}-selection-decision", run_id.as_str()),
            "selection-decision",
            &live_selection_report(&live_selection, historical_selection.len()),
            &mut handles,
        )?;
        put_json_run_artifact(
            &store,
            &run_id,
            &format!("artifact-{}-protection-graph", run_id.as_str()),
            "weavatrix-protection-graph",
            &protection_graph,
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
            &live_selection.bindings,
            &records,
            &run_id,
            &browser_runs,
            &cmd.evidence_policy,
        )?;
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
        persist_browser_runs(
            &store,
            &run_id,
            &browser_runs,
            &cmd.evidence_policy,
            &mut handles,
        )?;
        if let Some(base_replay) = base_browser_replay {
            persist_delta_triangle(
                &store,
                &run_id,
                &compiled,
                &changed,
                &graph_diff,
                &browser_runs,
                base_replay,
                &cmd.evidence_policy,
                &mut handles,
            )?;
        }
        persist_ui_integrity(
            &store,
            &run_id,
            &before,
            &ui_policy,
            &browser_runs,
            &mut handles,
        )?;
        let behavior =
            persist_browser_behavior(&store, &run_id, &before, &browser_runs, &mut handles)?;
        let test_analytics =
            persist_test_analytics(&store, &run_id, &before, &records, &browser_runs)?;
        put_run_artifact(
            &store,
            &run_id,
            &format!("artifact-{}-test-analytics", run_id.as_str()),
            "test-analytics",
            &test_analytics.bytes,
            &mut handles,
        )?;
        persist_dynamic_coverage_history(&store, &run_id, &before, &protection_graph, &records)?;
        if let Some(protection) = live_protection_snapshot(
            &self.repo,
            &before,
            &protection_graph,
            &records,
            &live_selection.bindings,
        )? {
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
            &scope_reason,
            &cmd.evidence_policy,
            outcome,
            &records,
            &browser_runs,
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

        let selected_test_count = executed_tests
            .as_ref()
            .map_or(available_test_count, BTreeSet::len);

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
            base_commit: range.base_commit,
            head_commit: range.head_commit,
            merge_base: range.merge_base,
            requested_scope: cmd.scope.clone(),
            scope: effective_scope,
            scope_reason,
            status: "complete".into(),
            executed: true,
            outcome: outcome.into(),
            selected_test_count: u64::try_from(selected_test_count).unwrap_or(u64::MAX),
            available_test_count: u64::try_from(available_test_count).unwrap_or(u64::MAX),
            executor_invocations: u64::try_from(records.len()).unwrap_or(u64::MAX),
            browser_programs: u64::try_from(browser_runs.len()).unwrap_or(u64::MAX),
            behavior_state_count: behavior.states,
            new_behavior_state_count: behavior.new_states,
            behavior_edge_count: behavior.edges,
            new_behavior_edge_count: behavior.new_edges,
            recorded_test_count: test_analytics.recorded_test_count,
            failed_test_count: test_analytics.failed_test_count,
            flaky_test_count: test_analytics.flaky_test_count,
            unknown_failure_count: test_analytics.unknown_failure_count,
            artifact_handles: handles,
        })
    }

    fn audit_selection(
        &self,
        impacted_run: &str,
        full_run: &str,
    ) -> Result<SelectionAuditReply, BusError> {
        audit_live_selection(&self.repo, &self.store()?, impacted_run, full_run)
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
        let mut obligation_execution = BTreeMap::<String, Vec<StoredObligationExecution>>::new();
        let mut browser_evidence = BTreeMap::<String, BrowserProofEvidence>::new();
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
            if record.kind == "browser-program-evidence" {
                merge_browser_proof_evidence(&mut browser_evidence, &bytes)?;
            }
        }
        let mut proofs = Vec::new();
        let mut verdicts = Vec::new();
        let mut outcomes = Vec::new();
        for obligation in &compiled.obligations {
            let proof_suffix = run.as_ref().map_or_else(
                || sha256_hex(revision.as_str().as_bytes())[..16].to_owned(),
                |run| run.id.to_string(),
            );
            let id = ProofId::new(format!("proof-{}-{proof_suffix}", obligation.id))
                .map_err(|err| BusError::Identity(err.to_string()))?;
            let browser = browser_evidence.get(obligation.id.as_str());
            let mut obligation_present = present.clone();
            if let Some(browser) = browser {
                for kind in &browser.present {
                    if !obligation_present.contains(kind) {
                        obligation_present.push(*kind);
                    }
                }
            }
            let exact = obligation_execution
                .get(obligation.id.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            let contradicted = exact.iter().any(|entry| entry.status == "contradicted")
                || browser.is_some_and(|evidence| evidence.contradicted);
            let failed = exact.iter().any(|entry| {
                !entry.invocation_passed || matches!(entry.status.as_str(), "failed" | "error")
            }) || browser.is_some_and(|evidence| evidence.failed);
            let passed = exact
                .iter()
                .any(|entry| entry.invocation_passed && entry.status == "passed")
                || browser.is_some_and(|evidence| evidence.passed);
            let execution = if contradicted {
                ExecutionEvidence::Failed {
                    seal_contradicted: true,
                    present: obligation_present,
                }
            } else if failed {
                ExecutionEvidence::Failed {
                    seal_contradicted: false,
                    present: obligation_present,
                }
            } else if passed {
                ExecutionEvidence::Passed {
                    present: obligation_present,
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
                program: browser
                    .filter(|evidence| evidence.programs.len() == 1)
                    .and_then(|evidence| evidence.programs.first())
                    .map(ProgramId::new)
                    .transpose()
                    .map_err(|err| BusError::Identity(err.to_string()))?,
                run: run.as_ref().map(|item| item.id.clone()),
                observations: browser
                    .map(|evidence| evidence.observations.clone())
                    .unwrap_or_default(),
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
                    .put_proof_with_artifacts(
                        &StoredProof {
                            id,
                            revision: revision.clone(),
                            obligation: obligation.id.clone(),
                            oracle_seal: oracle.id.clone(),
                            verdict: assembled.proof.verdict.as_str().into(),
                        },
                        &assembled.proof.artifacts,
                    )
                    .map_err(|err| BusError::Store(err.to_string()))?;
            }
            verdicts.push(assembled.proof.verdict);
            outcomes.push(ProofOutcome {
                obligation: obligation.id.to_string(),
                requirement: obligation.requirement.to_string(),
                verdict: assembled.proof.verdict,
                mandatory: matches!(obligation.risk, RiskLevel::High | RiskLevel::Critical),
            });
            proofs.push(ProofSummary {
                id: assembled.proof.id.to_string(),
                requirement: obligation.requirement.to_string(),
                obligation: obligation.id.to_string(),
                verdict: assembled.proof.verdict.as_str().to_owned(),
            });
        }
        let quality = compose(&self.verdict_inputs(&compiled, run.as_ref(), outcomes)?);
        Ok(combine_verify(&compiled.change, proofs, &verdicts, quality))
    }

    fn explain(&self, cmd: &ExplainCommand) -> Result<ExplainReply, BusError> {
        let store = self.store()?;
        if let Some(reply) = explain_ui_finding(&store, &cmd.id)? {
            return Ok(reply);
        }
        if let Ok(id) = ProofId::new(&cmd.id)
            && let Some(reply) = explain_stored_proof(&store, &id, &cmd.id)?
        {
            return Ok(reply);
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
            &json!({"base_ref": range.merge_base, "debt": "all", "max_findings": 5000}),
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
                "base_ref": range.merge_base,
                "max_tests": 500,
                "depth": 6,
                "max_nodes": 2000
            }),
        )?;
        let diff = self.weavatrix_operation(
            &revision,
            "graph_diff",
            &json!({
                "base_ref": range.merge_base,
                "detail": "edges",
                "max_results": 2000,
                "token_budget": 20000
            }),
        )?;
        let change_impact = self.weavatrix_operation(
            &revision,
            "change_impact",
            &json!({
                "base_ref": range.merge_base,
                "depth": 6,
                "max_nodes": 100_000,
                "precision": "graph"
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
        let store = self.store()?;
        let browser_bindings = load_live_browser_policy(&self.repo, &compiled, &store)?
            .as_ref()
            .map_or_else(Vec::new, browser_test_bindings);
        let impact = live_impacted_surface(&diff, &change_impact)?;
        let historical_selection = historical_selection_candidates(&store, &impact)?;
        let selection = build_live_selection(
            &self.repo,
            &static_report,
            &diff,
            &impact,
            &obligations,
            &browser_bindings,
            &historical_selection,
        )?;
        let selection_complete = selection.complete();
        Ok(SelectReply {
            base: range.base_ref,
            head: range.head_ref,
            revision: Some(revision.to_string()),
            algorithm: "weavatrix-base-head-history-union+greedy-weighted-set-cover".into(),
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

    fn author_draft(&self, cmd: &AuthorDraftCommand) -> Result<AuthorDraftReply, BusError> {
        validate_authoring_budget(cmd.token_budget)?;
        let compiled = self.compiled(&cmd.change)?;
        let range = self.revision_range(&cmd.base, &cmd.head)?;
        let changed = changed_files(&self.repo, &range)?;
        if changed.is_empty() {
            return Err(BusError::Intelligence(format!(
                "revision range `{}` -> `{}` contains no changed code to author against",
                cmd.base, cmd.head
            )));
        }
        let revision = self.revision()?;
        let graph_diff = self.weavatrix_operation(
            &revision,
            "graph_diff",
            &json!({
                "base_ref": range.merge_base,
                "detail": "edges",
                "max_results": 100_000
            }),
        )?;
        ensure_complete_diff(&graph_diff)?;
        let change_impact = self.weavatrix_operation(
            &revision,
            "change_impact",
            &json!({
                "base_ref": range.merge_base,
                "depth": 6,
                "max_nodes": 100_000,
                "precision": "graph"
            }),
        )?;
        if change_impact
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(BusError::Intelligence(
                "change_impact was truncated; refusing a partial authoring packet".into(),
            ));
        }

        let changed_files = changed.all();
        let obligations = authoring_obligations(&compiled.obligations)?;
        let authority_tokens = authoring_authority_tokens(&changed_files, &obligations)?;
        if authority_tokens >= cmd.token_budget {
            return Err(BusError::Runtime(format!(
                "authoring token budget {} cannot contain the complete sealed authority (needs more than {authority_tokens})",
                cmd.token_budget
            )));
        }
        let context_items =
            authoring_context(&compiled.spec, &changed_files, &graph_diff, &change_impact);
        let (context, context_tokens, truncated) = bound_items(
            context_items,
            cmd.token_budget.saturating_sub(authority_tokens),
        );
        let mut reply = AuthorDraftReply {
            change: compiled.change.clone(),
            revision: revision.to_string(),
            base: range.base_ref,
            head: range.head_ref,
            changed_files,
            context,
            obligations,
            truncated,
            tokens_used: authority_tokens.saturating_add(context_tokens),
            token_budget: cmd.token_budget,
            candidate: None,
            model_usage: None,
        };

        if cmd.use_model {
            let model = self.model(&ModelCommand {
                change: compiled.change.clone(),
                kind: "planning".into(),
                prompt: authoring_model_prompt(&reply)?,
            })?;
            let candidate: Value = serde_json::from_str(&model.text).map_err(|err| {
                BusError::Model(format!(
                    "authoring model did not return one strict TestProgram JSON object: {err}"
                ))
            })?;
            let validated = validate_author_candidate(&self.repo, &compiled, &candidate)?;
            reply.candidate = Some(
                serde_json::to_value(&validated.program)
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
            );
            reply.model_usage = Some(AuthorModelUsage {
                model: model.model,
                input_tokens: model.input_tokens,
                output_tokens: model.output_tokens,
                cost_micros: model.cost_micros,
            });
        }
        Ok(reply)
    }

    fn author_validate(
        &self,
        cmd: &AuthorValidateCommand,
    ) -> Result<AuthorValidateReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let validated = validate_author_candidate(&self.repo, &compiled, &cmd.program)?;
        let program = serde_json::to_value(&validated.program)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        Ok(AuthorValidateReply {
            change: compiled.change,
            seal_id: validated.seal_id,
            program_id: validated.program.id.to_string(),
            program,
            obligations: validated
                .program
                .obligations
                .iter()
                .map(ToString::to_string)
                .collect(),
            valid: true,
            persisted: false,
        })
    }

    fn author_preview_controlled(
        &self,
        cmd: &AuthorPreviewCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorPreviewReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let _range = self.revision_range(&cmd.base, &cmd.head)?;
        let before = self.revision()?;
        let validated = validate_author_candidate(&self.repo, &compiled, &cmd.program)?;
        let canonical_program = validated.program.clone();
        let canonical_program_body = serde_json::to_vec(&canonical_program)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        let seal_id = validated.seal_id.clone();
        let policy = load_browser_policy(&self.repo, &compiled.obligations)?.ok_or_else(|| {
            BusError::Runtime(
                "authoring preview requires a browser runtime in .weavatrix-quality/config.yaml"
                    .into(),
            )
        })?;
        let mut program = validated.program;
        program.evidence_policy.screenshot = if cmd.screenshot {
            CaptureWhen::Always
        } else {
            CaptureWhen::Never
        };
        program.evidence_policy.trace = if cmd.trace {
            CaptureWhen::Always
        } else {
            CaptureWhen::Never
        };
        let preview_token = author_preview_token(program.id.as_str())?;
        let evidence_dir = self
            .repo
            .join(".weavatrix-quality")
            .join("authoring-evidence")
            .join(&preview_token);
        let result = run_browser_program(
            &BrowserRunConfig {
                base_url: policy.base_url,
                browser: policy.browser,
                headless: policy.headless,
                timeout: policy.timeout,
                module_root: policy.module_root,
                runtime_dir: self
                    .repo
                    .join(".weavatrix-quality/runtime/playwright-runner"),
                evidence_dir: evidence_dir.clone(),
                viewport: None,
                // Authoring exercises one candidate program in isolation; UI
                // integrity is a base/head comparison with nothing to compare.
                ui_integrity: None,
                cancel,
            },
            &program,
            &validated.oracles,
        )
        .map_err(|err| BusError::Runtime(err.to_string()))?;
        let after = self.revision()?;
        if before != after {
            return Err(BusError::Ambiguous(format!(
                "repository revision changed during authoring preview: `{before}` -> `{after}`"
            )));
        }
        let store = self.store()?;
        let persisted = persist_author_preview(&store, &preview_token, &result)?;
        store
            .put_authoring_preview(
                &preview_token,
                canonical_program.id.as_str(),
                &compiled.change,
                before.as_str(),
                &seal_id,
                result.passed,
                &canonical_program_body,
            )
            .map_err(|err| BusError::Store(err.to_string()))?;
        let _ = std::fs::remove_dir(&evidence_dir);
        Ok(AuthorPreviewReply {
            preview_id: preview_token,
            change: compiled.change,
            revision: before.to_string(),
            program_id: program.id.to_string(),
            passed: result.passed,
            asserted: result.asserted,
            contradicted: result.contradicted,
            failure: result.failure,
            observation_handles: persisted.observation_handles,
            screenshot_handles: persisted.screenshot_handles,
            trace_handle: persisted.trace_handle,
            program_persisted: false,
        })
    }

    fn author_promote(&self, cmd: &AuthorPromoteCommand) -> Result<AuthorPromoteReply, BusError> {
        if cmd.preview_id.trim().is_empty() {
            return Err(BusError::InvalidInput(
                "preview_id must not be empty".into(),
            ));
        }
        let compiled = self.compiled(&cmd.change)?;
        let repository_revision = self.revision()?;
        let validated = validate_author_candidate(&self.repo, &compiled, &cmd.program)?;
        let program_body = serde_json::to_vec(&validated.program)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        let mut store = self.store()?;
        let (program_revision, created) = store
            .promote_authoring_preview(
                &cmd.preview_id,
                validated.program.id.as_str(),
                &compiled.change,
                repository_revision.as_str(),
                &validated.seal_id,
                &program_body,
            )
            .map_err(map_authoring_store_error)?;
        Ok(AuthorPromoteReply {
            change: compiled.change,
            revision: repository_revision.to_string(),
            seal_id: validated.seal_id,
            program_id: validated.program.id.to_string(),
            program_revision,
            persisted: true,
            created,
        })
    }

    #[allow(clippy::too_many_lines)]
    fn author_heal_controlled(
        &self,
        cmd: &AuthorHealCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorHealReply, BusError> {
        if cmd.program_id.trim().is_empty() || cmd.expected_program_revision == 0 {
            return Err(BusError::InvalidInput(
                "healing requires a program id and positive expected revision".into(),
            ));
        }
        if cmd.edits.is_empty() || cmd.edits.len() > 64 {
            return Err(BusError::InvalidInput(
                "healing requires between 1 and 64 bounded edits".into(),
            ));
        }
        let compiled = self.compiled(&cmd.change)?;
        let _range = self.revision_range(&cmd.base, &cmd.head)?;
        let before = self.revision()?;
        let mut store = self.store()?;
        let latest = store
            .latest_program_revision(&cmd.program_id)
            .map_err(|err| BusError::Store(err.to_string()))?
            .ok_or_else(|| BusError::NotFound(format!("browser TestProgram {}", cmd.program_id)))?;
        if latest != cmd.expected_program_revision {
            return Err(BusError::Ambiguous(format!(
                "program revision changed: expected {}, latest is {latest}",
                cmd.expected_program_revision
            )));
        }
        let (stored, body) = store
            .read_program_revision(&cmd.program_id, latest)
            .map_err(|err| BusError::Store(err.to_string()))?
            .ok_or_else(|| {
                BusError::NotFound(format!(
                    "browser TestProgram {} revision {latest}",
                    cmd.program_id
                ))
            })?;
        if stored.change_id != compiled.change {
            return Err(BusError::InvalidInput(
                "healing cannot move a program to another change".into(),
            ));
        }
        let candidate: Value = serde_json::from_slice(&body).map_err(|err| {
            BusError::Store(format!(
                "stored TestProgram {} revision {latest} is malformed: {err}",
                cmd.program_id
            ))
        })?;
        let validated = validate_author_candidate(&self.repo, &compiled, &candidate)?;
        if stored.seal != validated.seal_id {
            return Err(BusError::InvalidInput(
                "OracleSeal changed since promotion; a contradiction is not healable".into(),
            ));
        }
        let stored_seal =
            OracleSealId::new(&stored.seal).map_err(|err| BusError::Identity(err.to_string()))?;
        let current_seal = OracleSealId::new(&validated.seal_id)
            .map_err(|err| BusError::Identity(err.to_string()))?;
        let edits = cmd
            .edits
            .iter()
            .cloned()
            .map(|edit| match edit {
                AuthorHealEdit::Retarget { step, target } => HealEdit::Retarget { step, target },
                AuthorHealEdit::InsertWait { after, condition } => {
                    HealEdit::InsertWait { after, condition }
                }
            })
            .collect::<Vec<_>>();
        let healed = apply_heal(
            &validated.program,
            &stored_seal,
            &current_seal,
            &edits,
            latest,
        )
        .map_err(|err| BusError::InvalidInput(err.to_string()))?;
        let canonical_program = healed.program;
        let canonical_program_body = serde_json::to_vec(&canonical_program)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        let policy = load_browser_policy(&self.repo, &compiled.obligations)?.ok_or_else(|| {
            BusError::Runtime(
                "safe healing requires a browser runtime in .weavatrix-quality/config.yaml".into(),
            )
        })?;
        let mut executable = canonical_program.clone();
        executable.evidence_policy.screenshot = if cmd.screenshot {
            CaptureWhen::Always
        } else {
            CaptureWhen::Never
        };
        executable.evidence_policy.trace = if cmd.trace {
            CaptureWhen::Always
        } else {
            CaptureWhen::Never
        };
        let preview_token = author_preview_token(&format!("heal-{}", cmd.program_id))?;
        let evidence_dir = self
            .repo
            .join(".weavatrix-quality")
            .join("authoring-evidence")
            .join(&preview_token);
        let result = run_browser_program(
            &BrowserRunConfig {
                base_url: policy.base_url,
                browser: policy.browser,
                headless: policy.headless,
                timeout: policy.timeout,
                module_root: policy.module_root,
                runtime_dir: self
                    .repo
                    .join(".weavatrix-quality/runtime/playwright-runner"),
                evidence_dir: evidence_dir.clone(),
                viewport: None,
                // Authoring exercises one candidate program in isolation; UI
                // integrity is a base/head comparison with nothing to compare.
                ui_integrity: None,
                cancel,
            },
            &executable,
            &validated.oracles,
        )
        .map_err(|err| BusError::Runtime(err.to_string()))?;
        let after = self.revision()?;
        if before != after {
            return Err(BusError::Ambiguous(format!(
                "repository revision changed during safe-healing replay: `{before}` -> `{after}`"
            )));
        }
        let persisted = persist_author_preview(&store, &preview_token, &result)?;
        store
            .put_authoring_preview(
                &preview_token,
                canonical_program.id.as_str(),
                &compiled.change,
                before.as_str(),
                &validated.seal_id,
                result.passed,
                &canonical_program_body,
            )
            .map_err(|err| BusError::Store(err.to_string()))?;
        let (program_revision, created) = if result.passed {
            let (revision, created) = store
                .heal_authoring_preview(
                    &preview_token,
                    canonical_program.id.as_str(),
                    latest,
                    &compiled.change,
                    before.as_str(),
                    &validated.seal_id,
                    &canonical_program_body,
                )
                .map_err(map_authoring_store_error)?;
            (Some(revision), created)
        } else {
            (None, false)
        };
        let did_persist = program_revision.is_some();
        let _ = std::fs::remove_dir(&evidence_dir);
        Ok(AuthorHealReply {
            preview_id: preview_token,
            change: compiled.change,
            revision: before.to_string(),
            seal_id: validated.seal_id,
            program_id: canonical_program.id.to_string(),
            previous_program_revision: latest,
            program_revision,
            passed: result.passed,
            asserted: result.asserted,
            contradicted: result.contradicted,
            failure: result.failure,
            observation_handles: persisted.observation_handles,
            screenshot_handles: persisted.screenshot_handles,
            trace_handle: persisted.trace_handle,
            persisted: did_persist,
            created,
        })
    }

    fn changes(&self, cmd: &ChangesCommand) -> Result<ChangesReply, BusError> {
        let _ = cmd;
        Ok(ChangesReply {
            changes: list_changes(&self.repo)?,
        })
    }

    fn recovery(&self, cmd: &RecoveryCommand) -> Result<RecoveryReply, BusError> {
        let desk = self.recovery_desk(&cmd.change, &cmd.base, &cmd.head)?;
        let packet = desk.packet().cloned().ok_or_else(|| {
            BusError::Intelligence("recovery producer omitted its evidence packet".into())
        })?;
        Ok(RecoveryReply {
            packet,
            review: desk.review(),
            questions: desk.questions(),
            proposed_patch: desk.preview_patch(),
            runtime_llm_tokens: 0,
        })
    }
}

struct ValidatedAuthorProgram {
    program: TestProgram,
    oracles: Vec<ProgramOracle>,
    seal_id: String,
}

fn map_authoring_store_error(err: StoreError) -> BusError {
    match err {
        StoreError::Invalid(message) => BusError::InvalidInput(message),
        other => BusError::Store(other.to_string()),
    }
}

fn validate_authoring_budget(budget: u64) -> Result<(), BusError> {
    if (256..=64_000).contains(&budget) {
        Ok(())
    } else {
        Err(BusError::Unknown {
            field: "token_budget",
            value: budget.to_string(),
        })
    }
}

fn authoring_obligations(
    obligations: &[TestObligation],
) -> Result<Vec<AuthoringObligation>, BusError> {
    obligations
        .iter()
        .map(|item| {
            Ok(AuthoringObligation {
                id: item.id.to_string(),
                requirement: item.requirement.to_string(),
                scenario: item.scenario.to_string(),
                kind: obligation_kind_token(item.kind).into(),
                risk: risk_token(item.risk).into(),
                condition: item
                    .condition
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
                expected: item
                    .expected
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
                required_evidence: item
                    .required_evidence
                    .iter()
                    .map(|kind| evidence_kind_token(*kind).to_owned())
                    .collect(),
            })
        })
        .collect()
}

fn authoring_authority_tokens(
    changed_files: &[String],
    obligations: &[AuthoringObligation],
) -> Result<u64, BusError> {
    let authority = serde_json::to_string(&json!({
        "changed_files": changed_files,
        "obligations": obligations,
    }))
    .map_err(|err| BusError::Runtime(err.to_string()))?;
    Ok(estimate_tokens(&authority).max(1))
}

fn authoring_context(
    spec: &OpenSpecChange,
    changed_files: &[String],
    diff: &Value,
    impact: &Value,
) -> Vec<String> {
    let mut out = detailed_requirement_texts(spec);
    out.extend(
        changed_files
            .iter()
            .map(|path| format!("changed file: {path}")),
    );
    for (label, pointer) in [
        ("graph added", "/nodes/added"),
        ("graph removed", "/nodes/removed"),
    ] {
        out.extend(
            values_at(diff, pointer)
                .iter()
                .filter_map(graph_node_id)
                .map(|id| format!("{label}: {id}")),
        );
    }
    for item in values_at(diff, "/nodes/changed") {
        if let Some(id) = item.get("before").and_then(graph_node_id) {
            out.push(format!("graph changed base: {id}"));
        }
        if let Some(id) = item.get("after").and_then(graph_node_id) {
            out.push(format!("graph changed head: {id}"));
        }
    }
    out.extend(
        values_at(impact, "/impacted_nodes")
            .iter()
            .filter_map(graph_node_id)
            .map(|id| format!("graph impacted: {id}")),
    );
    out.sort();
    out.dedup();
    out
}

fn detailed_requirement_texts(spec: &OpenSpecChange) -> Vec<String> {
    let mut out = Vec::new();
    for capability in &spec.capabilities {
        for operation in &capability.operations {
            let delta = match operation {
                RequirementOp::Added(delta)
                | RequirementOp::Modified(delta)
                | RequirementOp::Removed(delta) => delta,
                RequirementOp::Renamed { from, to, location } => {
                    out.push(format!(
                        "intent rename at {}:{}: {from} -> {to}",
                        location.file.display(),
                        location.line
                    ));
                    continue;
                }
            };
            out.push(format!(
                "intent requirement {}: {} — {}",
                delta.id, delta.name, delta.text
            ));
            for scenario in &delta.scenarios {
                let clauses = scenario
                    .clauses
                    .iter()
                    .map(|clause| format!("{:?} {}", clause.kind, clause.text))
                    .collect::<Vec<_>>()
                    .join("; ");
                out.push(format!(
                    "intent scenario {} ({}) for {}: {clauses}",
                    scenario.id, scenario.name, delta.id
                ));
            }
        }
    }
    out
}

fn authoring_model_prompt(reply: &AuthorDraftReply) -> Result<String, BusError> {
    let input = serde_json::to_value(reply).map_err(|err| BusError::Runtime(err.to_string()))?;
    serde_json::to_string(&json!({
        "task": "Return exactly one JSON object containing a canonical schema_v=1 TestProgram. Do not use markdown.",
        "rules": [
            "source must be generated",
            "only assert obligation ids whose expected field is non-null",
            "every declared obligation must have an assert step",
            "prefer semantic targets: test_id, role plus accessible_name, or label",
            "routes and api operation paths must be same-origin root-relative",
            "never invent an oracle, expected predicate, shell command, XPath, JavaScript, or filesystem write",
            "use only navigate, activate, fill, select, press, wait, set_feature_flag, inject_fault, api_call, assert"
        ],
        "test_program_shape": {
            "schema_v": 1,
            "id": "generated-program-id",
            "source": "generated",
            "obligations": ["sealed-obligation-id"],
            "preconditions": [],
            "steps": [{"action": "navigate", "route": "/"}, {"action": "assert", "obligation": "sealed-obligation-id"}],
            "data": {},
            "faults": {},
            "api_operations": {},
            "evidence_policy": {"screenshot": "on_failure", "trace": "on_failure", "network": "always", "console": "always", "storage": "on_failure"},
            "deterministic_seed": 1
        },
        "authoring_packet": input
    }))
    .map_err(|err| BusError::Runtime(err.to_string()))
}

fn validate_author_candidate(
    repo: &Path,
    compiled: &Compiled,
    candidate: &Value,
) -> Result<ValidatedAuthorProgram, BusError> {
    if !candidate.is_object() {
        return Err(BusError::InvalidInput(
            "authoring candidate must be one TestProgram JSON object".into(),
        ));
    }
    let raw = serde_json::to_string(candidate).map_err(|err| BusError::Runtime(err.to_string()))?;
    let program = TestProgram::from_json(&raw)
        .map_err(|err| BusError::InvalidInput(format!("invalid authoring candidate: {err}")))?;
    let mut unique = BTreeSet::new();
    if program
        .obligations
        .iter()
        .any(|obligation| !unique.insert(obligation.as_str()))
    {
        return Err(BusError::InvalidInput(format!(
            "authoring candidate {} repeats an obligation",
            program.id
        )));
    }
    let known = compiled
        .obligations
        .iter()
        .map(|obligation| (obligation.id.as_str(), obligation))
        .collect::<BTreeMap<_, _>>();
    let mut oracles = Vec::new();
    for obligation in &program.obligations {
        let sealed = known.get(obligation.as_str()).ok_or_else(|| {
            BusError::InvalidInput(format!(
                "authoring candidate {} names unknown obligation {obligation}",
                program.id
            ))
        })?;
        let expected = sealed.expected.as_ref().ok_or_else(|| {
            BusError::InvalidInput(format!(
                "authoring candidate {} cannot assert {obligation}: the existing seal has no executable expected predicate",
                program.id
            ))
        })?;
        oracles.push(ProgramOracle {
            obligation: obligation.clone(),
            condition: sealed
                .condition
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(|err| BusError::Runtime(err.to_string()))?,
            expected: serde_json::to_value(expected)
                .map_err(|err| BusError::Runtime(err.to_string()))?,
        });
    }
    let contract = load_quality_contract(repo, &compiled.change)?;
    let oracle_seal = seal(&contract, &compiled.obligations, &compiled.spec)?;
    Ok(ValidatedAuthorProgram {
        program,
        oracles,
        seal_id: oracle_seal.id.to_string(),
    })
}

fn author_preview_token(program: &str) -> Result<String, BusError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| BusError::Runtime(format!("system clock is before Unix epoch: {err}")))?
        .as_nanos();
    Ok(format!("{}-{nanos}", safe_file_token(program)))
}

struct PersistedAuthorPreview {
    observation_handles: Vec<String>,
    screenshot_handles: Vec<String>,
    trace_handle: Option<String>,
}

fn persist_author_preview(
    store: &Store,
    token: &str,
    result: &BrowserProgramRun,
) -> Result<PersistedAuthorPreview, BusError> {
    let mut artifacts = Vec::<(String, String, Vec<u8>)>::new();
    let mut observation_handles = Vec::new();
    for (index, observation) in result.observations.iter().enumerate() {
        let id = format!("artifact-author-{token}-observation-{index}");
        let bytes =
            serde_json::to_vec(observation).map_err(|err| BusError::Store(err.to_string()))?;
        artifacts.push((id.clone(), "browser-observation".into(), bytes));
        observation_handles.push(id);
    }
    let mut screenshot_handles = Vec::new();
    for (index, path) in result.screenshot_paths.iter().enumerate() {
        let bytes = std::fs::read(path).map_err(|err| {
            BusError::Runtime(format!(
                "cannot import authoring screenshot {}: {err}",
                path.display()
            ))
        })?;
        remove_browser_evidence_file(path)?;
        let id = format!("artifact-author-{token}-screenshot-{index}");
        artifacts.push((id.clone(), "screenshot".into(), bytes));
        screenshot_handles.push(id);
    }
    let trace_handle = if let Some(path) = &result.trace_path {
        let bytes = std::fs::read(path).map_err(|err| {
            BusError::Runtime(format!(
                "cannot import authoring trace {}: {err}",
                path.display()
            ))
        })?;
        remove_browser_evidence_file(path)?;
        let id = format!("artifact-author-{token}-trace");
        artifacts.push((id.clone(), "playwright-trace".into(), bytes));
        Some(id)
    } else {
        None
    };
    for (raw_id, kind, bytes) in artifacts {
        let id = ArtifactId::new(&raw_id).map_err(|err| BusError::Identity(err.to_string()))?;
        store
            .put_artifact(&id, &kind, &bytes)
            .map_err(|err| BusError::Store(err.to_string()))?;
    }
    Ok(PersistedAuthorPreview {
        observation_handles,
        screenshot_handles,
        trace_handle,
    })
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

fn evidence_kind_token(kind: EvidenceKind) -> &'static str {
    match kind {
        EvidenceKind::Dom => "dom",
        EvidenceKind::Network => "network",
        EvidenceKind::Screenshot => "screenshot",
        EvidenceKind::Trace => "trace",
        EvidenceKind::Har => "har",
        EvidenceKind::Console => "console",
        EvidenceKind::Storage => "storage",
        EvidenceKind::Coverage => "coverage",
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

fn git_output(repo: &Path, args: &[String]) -> Result<Vec<u8>, BusError> {
    let output = ProcessCommand::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|err| BusError::Intelligence(format!("cannot run Git: {err}")))?;
    if !output.status.success() {
        return Err(BusError::Intelligence(format!(
            "Git {} failed: {}",
            args.first().map_or("operation", String::as_str),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

fn changed_files(repo: &Path, range: &RevisionRange) -> Result<ChangedFiles, BusError> {
    let mut args = vec![
        "diff".into(),
        "--name-status".into(),
        "-M".into(),
        range.merge_base.clone(),
    ];
    if range.head_ref != "WORKTREE" {
        args.push(range.head_commit.clone());
    }
    args.push("--".into());
    let raw = String::from_utf8(git_output(repo, &args)?)
        .map_err(|err| BusError::Intelligence(format!("Git diff paths are not UTF-8: {err}")))?;
    let mut out = ChangedFiles::default();
    for line in raw.lines().filter(|line| !line.is_empty()) {
        let fields = line.split('\t').collect::<Vec<_>>();
        let status = fields.first().copied().unwrap_or_default();
        match status.chars().next() {
            Some('A') if fields.len() >= 2 => out.added.push(normalize_path(fields[1])),
            Some('D') if fields.len() >= 2 => out.removed.push(normalize_path(fields[1])),
            Some('R' | 'C') if fields.len() >= 3 => {
                out.removed.push(normalize_path(fields[1]));
                out.added.push(normalize_path(fields[2]));
            }
            Some(_) if fields.len() >= 2 => out.changed.push(normalize_path(fields[1])),
            _ => {
                return Err(BusError::Intelligence(format!(
                    "cannot decode Git name-status row `{line}`"
                )));
            }
        }
    }
    if range.head_ref == "WORKTREE" {
        let untracked = String::from_utf8(git_output(
            repo,
            &[
                "ls-files".into(),
                "--others".into(),
                "--exclude-standard".into(),
            ],
        )?)
        .map_err(|err| BusError::Intelligence(format!("Git paths are not UTF-8: {err}")))?;
        out.added.extend(
            untracked
                .lines()
                .filter(|line| !line.is_empty())
                .map(normalize_path),
        );
    }
    for list in [&mut out.added, &mut out.changed, &mut out.removed] {
        list.sort();
        list.dedup();
    }
    Ok(out)
}

fn protection_graph_for_files(
    repo: &Path,
    revision: &RevisionId,
    files: &[String],
) -> Result<Value, BusError> {
    let indexed = WeavatrixProvider
        .indexed_files(repo)
        .map_err(|err| BusError::Intelligence(err.to_string()))?;
    let seeds = files
        .iter()
        .filter(|path| repo.join(path).is_file() && indexed.contains(path.as_str()))
        .map(|path| format!("file:{path}"))
        .collect::<Vec<_>>();
    if seeds.is_empty() {
        return Ok(json!({"nodes": [], "edges": [], "revision": revision.as_str()}));
    }
    let report = WeavatrixProvider
        .operation(
            repo,
            "query_graph",
            &json!({
                "seed_files": seeds,
                "depth": 8,
                "max_nodes": 100_000,
                "flow_direction": "both",
                "mode": "bfs"
            }),
        )
        .map_err(|err| BusError::Intelligence(err.to_string()))?;
    let found = report
        .get("revision")
        .and_then(Value::as_str)
        .ok_or_else(|| BusError::Intelligence("query_graph omitted revision identity".into()))?;
    if found != revision.as_str() {
        return Err(BusError::Ambiguous(format!(
            "query_graph evidence belongs to revision `{found}`, expected `{revision}`"
        )));
    }
    if report
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(BusError::Intelligence(
            "protection graph exceeded its bounded query; refusing partial coverage mapping".into(),
        ));
    }
    let mut nodes = report
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("node").cloned())
        .collect::<Vec<_>>();
    nodes.sort_by_key(graph_node_id);
    nodes.dedup_by(|left, right| graph_node_id(left) == graph_node_id(right));
    Ok(json!({
        "nodes": nodes,
        "edges": report.get("edges").cloned().unwrap_or_else(|| json!([])),
        "revision": revision.as_str()
    }))
}

fn recovery_code_delta(diff: &Value) -> (CodeDeltaSummary, PublicSurfaceDelta) {
    let added = values_at(diff, "/nodes/added");
    let removed = values_at(diff, "/nodes/removed");
    let changed = values_at(diff, "/nodes/changed");
    let mut changed_nodes = Vec::new();
    changed_nodes.extend(added.iter());
    changed_nodes.extend(removed.iter());
    for item in changed {
        changed_nodes.extend(item.get("before"));
        changed_nodes.extend(item.get("after"));
    }
    let mut changed_symbols = changed_nodes
        .iter()
        .filter_map(|node| graph_node_id(node))
        .collect::<Vec<_>>();
    changed_symbols.sort();
    changed_symbols.dedup();
    let mut public_symbols = changed_nodes
        .iter()
        .filter(|node| graph_node_is_public_function(node))
        .filter_map(|node| recovery_public_symbol_id(node))
        .collect::<Vec<_>>();
    public_symbols.sort();
    public_symbols.dedup();
    let mut components = changed_nodes
        .iter()
        .filter(|node| {
            node.get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.to_ascii_lowercase().contains("component"))
        })
        .filter_map(|node| graph_node_id(node))
        .collect::<Vec<_>>();
    components.sort();
    components.dedup();
    let surfaces = PublicSurfaceDelta {
        added: surface_labels(added),
        removed: surface_labels(removed),
    };
    (
        CodeDeltaSummary {
            components,
            endpoints_added: surfaces.added.clone(),
            endpoints_removed: surfaces.removed.clone(),
            changed_symbols,
            public_symbols,
        },
        surfaces,
    )
}

fn recovery_existing_requirements(repo: &Path, change: &str) -> Result<Vec<String>, BusError> {
    let path = repo.join("openspec").join("changes").join(change);
    if !path.is_dir() {
        return Ok(Vec::new());
    }
    let spec = read_change(repo, change)?;
    Ok(requirement_texts(&spec))
}

fn recovery_evidence(
    repo: &Path,
    range: &RevisionRange,
    code: &CodeDeltaSummary,
    files: &ChangedFiles,
    existing_requirements: &[String],
) -> Result<Vec<IntentEvidence>, BusError> {
    let mut out = existing_requirements
        .iter()
        .map(|text| IntentEvidence::new(EvidenceSource::ExistingOpenSpec, text, "OpenSpec"))
        .collect::<Vec<_>>();
    for symbol in code.changed_symbols.iter().take(500) {
        out.push(IntentEvidence::new(
            EvidenceSource::CodeDelta,
            symbol,
            format!(
                "Weavatrix graph_diff {}..{}",
                range.merge_base, range.head_ref
            ),
        ));
    }
    for endpoint in code
        .endpoints_added
        .iter()
        .chain(code.endpoints_removed.iter())
    {
        out.push(IntentEvidence::new(
            EvidenceSource::ChangedEndpoint,
            endpoint,
            "Weavatrix public-surface delta",
        ));
    }
    for test in files.changed_tests() {
        out.push(IntentEvidence::new(
            EvidenceSource::ChangedTest,
            format!("test changed: {test}"),
            format!("Git diff {test}"),
        ));
    }
    let log = recovery_log(repo, range)?;
    for record in log {
        if !record.title.is_empty() {
            out.push(IntentEvidence::new(
                EvidenceSource::CommitTitle,
                record.title,
                format!("commit {}", record.id),
            ));
        }
        if !record.body.is_empty() {
            out.push(IntentEvidence::new(
                EvidenceSource::CommitBody,
                record.body,
                format!("commit {} body", record.id),
            ));
        }
    }
    Ok(out)
}

struct RecoveryLogRecord {
    id: String,
    title: String,
    body: String,
}

fn recovery_log(repo: &Path, range: &RevisionRange) -> Result<Vec<RecoveryLogRecord>, BusError> {
    let revset = format!("{}..{}", range.merge_base, range.head_commit);
    let raw = git_output(
        repo,
        &[
            "log".into(),
            "--reverse".into(),
            "--format=%H%x1f%s%x1f%b%x1e".into(),
            revset,
            "--".into(),
        ],
    )?;
    let raw = String::from_utf8(raw)
        .map_err(|err| BusError::Intelligence(format!("Git log is not UTF-8: {err}")))?;
    let mut records = Vec::new();
    for record in raw.split('\u{1e}') {
        let record = record.trim_matches(['\r', '\n']);
        if record.is_empty() {
            continue;
        }
        let mut fields = record.splitn(3, '\u{1f}');
        let id = fields.next().unwrap_or_default().trim().to_owned();
        let title = fields.next().unwrap_or_default().trim().to_owned();
        let body = fields.next().unwrap_or_default().trim().to_owned();
        if id.len() != 40 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(BusError::Intelligence(format!(
                "Git log returned an invalid commit id `{id}`"
            )));
        }
        records.push(RecoveryLogRecord { id, title, body });
    }
    Ok(records)
}

fn recovery_commits(
    repo: &Path,
    range: &RevisionRange,
    head_revision: &str,
    components: &[String],
    has_file_delta: bool,
) -> Result<Vec<CommitFacts>, BusError> {
    let mut facts = recovery_log(repo, range)?
        .into_iter()
        .enumerate()
        .map(|(index, record)| CommitFacts {
            id: record.id,
            title: record.title,
            index: u32::try_from(index).unwrap_or(u32::MAX),
            issue: linked_issue(&record.body),
            ..CommitFacts::default()
        })
        .collect::<Vec<_>>();
    if range.head_ref == "WORKTREE" && has_file_delta {
        facts.push(CommitFacts {
            id: head_revision.to_owned(),
            title: "working tree change".into(),
            index: u32::try_from(facts.len()).unwrap_or(u32::MAX),
            components: components.to_vec(),
            ..CommitFacts::default()
        });
    }
    Ok(facts)
}

fn linked_issue(text: &str) -> Option<String> {
    text.split(|character: char| !(character.is_ascii_alphanumeric() || character == '-'))
        .find(|token| {
            let Some((prefix, number)) = token.rsplit_once('-') else {
                return false;
            };
            !prefix.is_empty()
                && prefix
                    .chars()
                    .all(|character| character.is_ascii_uppercase())
                && !number.is_empty()
                && number.chars().all(|character| character.is_ascii_digit())
        })
        .map(ToOwned::to_owned)
}

fn recovery_candidates(
    surfaces: &PublicSurfaceDelta,
    code: &CodeDeltaSummary,
    evidence: &[IntentEvidence],
    recover_changed_symbols: bool,
) -> Vec<CandidateRequirement> {
    let mut subjects = surfaces
        .added
        .iter()
        .map(|surface| (surface.as_str(), true, "surface is available", false))
        .chain(
            surfaces
                .removed
                .iter()
                .map(|surface| (surface.as_str(), false, "surface is unavailable", false)),
        )
        .chain(
            code.components
                .iter()
                .map(|component| (component.as_str(), true, "component is visible", false)),
        )
        .chain(
            recover_changed_symbols
                .then_some(code.public_symbols.as_slice())
                .into_iter()
                .flatten()
                .map(|symbol| (symbol.as_str(), true, "", true)),
        )
        .collect::<Vec<_>>();
    subjects.sort_by_key(|(subject, expected, _, _)| ((*subject).to_owned(), *expected));
    subjects.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
    subjects
        .into_iter()
        .take(100)
        .enumerate()
        .map(|(index, (subject, expected_to_hold, outcome, changed_symbol))| {
            let lower = subject.to_ascii_lowercase();
            CandidateRequirement {
                id: format!("recovered-{}-{}", index + 1, recovery_slug(subject)),
                subject: subject.to_owned(),
                text: if changed_symbol {
                    "When a user exercises the affected public capability, the externally observable outcome SHALL match the behavior demonstrated by the changed test.".into()
                } else {
                    format!(
                        "When a user exercises `{subject}`, the externally observable {outcome}."
                    )
                },
                expected_to_hold,
                actor: Some("user".into()),
                precondition: Some("the changed capability is deployed".into()),
                trigger: Some(if changed_symbol {
                    "the user exercises the affected public capability".into()
                } else {
                    format!("the user exercises `{subject}`")
                }),
                endpoint: (surfaces.added.contains(&subject.to_owned())
                    || surfaces.removed.contains(&subject.to_owned()))
                .then(|| subject.to_owned()),
                evidence: evidence
                    .iter()
                    .filter(|item| {
                        item.text.contains(subject)
                            || (changed_symbol && item.source == EvidenceSource::ChangedTest)
                    })
                    .take(20)
                    .cloned()
                    .collect(),
                shape: if changed_symbol {
                    CandidateShape::default()
                } else {
                    CandidateShape {
                        numeric_limit: subject.chars().any(|character| character.is_ascii_digit()),
                        permission_sensitive: ["permission", "auth", "role", "admin", "viewer"]
                            .iter()
                            .any(|token| lower.contains(token)),
                        async_ui: ["async", "loading", "refresh", "request"]
                            .iter()
                            .any(|token| lower.contains(token)),
                    }
                },
                covered_cases: Vec::new(),
            }
        })
        .collect()
}

fn recovery_slug(value: &str) -> String {
    let mut out = String::new();
    let mut separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !out.is_empty() {
                out.push('-');
            }
            out.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
        if out.len() >= 48 {
            break;
        }
    }
    if out.is_empty() { "change".into() } else { out }
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
    let proofs = vec![ProofSummary {
        id: "proof-fake".into(),
        requirement: "sankey.visual-limit-others".into(),
        obligation: "others-visible".into(),
        verdict: verdict.to_owned(),
    }];
    let outcomes = vec![ProofOutcome {
        obligation: "others-visible".into(),
        requirement: "sankey.visual-limit-others".into(),
        verdict: parse_proof_verdict(verdict),
        mandatory: false,
    }];
    combine_verify(
        change,
        proofs,
        &[parse_proof_verdict(verdict)],
        compose(&VerdictInputs {
            proofs: outcomes,
            ..VerdictInputs::default()
        }),
    )
}

fn parse_proof_verdict(token: &str) -> ProofVerdict {
    match token {
        "PROVEN" => ProofVerdict::Proven,
        "CONTRADICTED" => ProofVerdict::Contradicted,
        "PARTIAL" => ProofVerdict::Partial,
        "HUMAN_REQUIRED" => ProofVerdict::HumanRequired,
        _ => ProofVerdict::Unproven,
    }
}

/// Explain one UI-integrity finding by fingerprint, detector id, or subject.
///
/// The reply is quantitative on purpose. A reviewer is told which control, on
/// which route, at which viewport, what covered or duplicated it, and the exact
/// hit-test or geometry numbers behind the call — never that something
/// "possibly overlaps".
fn explain_ui_finding(store: &Store, id: &str) -> Result<Option<ExplainReply>, BusError> {
    let looks_like_ui = id.starts_with("ui:") || id.starts_with("WVQ-UI-");
    if !looks_like_ui {
        return Ok(None);
    }
    let Some(run) = store
        .latest_run_any()
        .map_err(|err| BusError::Store(err.to_string()))?
    else {
        return Ok(None);
    };
    let Ok(document) = read_single_run_json(store, &run.id, "ui-integrity-findings") else {
        return Ok(None);
    };
    let findings: Vec<UiIntegrityFinding> =
        serde_json::from_value(document.get("findings").cloned().unwrap_or(json!([])))
            .map_err(|err| BusError::Store(format!("malformed ui findings: {err}")))?;
    let Some(finding) = findings
        .iter()
        .find(|item| item.fingerprint() == id)
        .or_else(|| findings.iter().find(|item| item.check.id() == id))
    else {
        return Ok(None);
    };

    let mut provenance = vec![
        format!("check {}", finding.check.id()),
        format!("fingerprint {}", finding.fingerprint()),
        format!("head revision {}", run.revision),
        format!("state {}", finding.state),
        format!("route {} at {}", finding.route, finding.viewport),
        format!("target {}", finding.subject),
    ];
    if let Some(counterpart) = &finding.counterpart {
        provenance.push(format!("counterpart {counterpart}"));
    }
    if let Some(component) = &finding.component_hint {
        provenance.push(format!("component {component}"));
    }
    let evidence = finding.evidence;
    if evidence.sample_count > 0 {
        provenance.push(format!(
            "hit tests {}/{} points received events ({} permille lost)",
            evidence.received_event_samples, evidence.sample_count, evidence.failure_ratio_permille
        ));
    }
    if evidence.overlap_ratio_permille > 0 {
        provenance.push(format!(
            "overlap {} permille of the target box",
            evidence.overlap_ratio_permille
        ));
    }
    if evidence.overflow_px != 0 {
        provenance.push(format!("overflow {}px", evidence.overflow_px));
    }
    if evidence.scroll_width != 0 || evidence.client_width != 0 {
        provenance.push(format!(
            "text {}x{} in a {}x{} box",
            evidence.scroll_width,
            evidence.scroll_height,
            evidence.client_width,
            evidence.client_height
        ));
    }
    if evidence.duplicate_count > 0 {
        provenance.push(format!("{} matching nodes", evidence.duplicate_count));
    }
    if !finding.nodes.is_empty() {
        provenance.push(format!("collector nodes {}", finding.nodes.join(", ")));
    }
    // The full snapshot and hit-test map stay handles; only their identity is
    // inlined so a caller can fetch them through `quality_evidence`.
    for kind in [
        "ui-layout-snapshot",
        "ui-hit-test-map",
        UI_INTEGRITY_DELTA_KIND,
    ] {
        if let Some(handle) = artifact_handle_of_kind(store, &run.id, kind)? {
            provenance.push(format!("artifact {kind} {handle}"));
        }
    }
    Ok(Some(ExplainReply {
        id: id.to_owned(),
        kind: "ui_finding".into(),
        summary: finding.detail.clone(),
        provenance,
    }))
}

fn artifact_handle_of_kind(
    store: &Store,
    run: &RunId,
    kind: &str,
) -> Result<Option<String>, BusError> {
    for artifact in store
        .run_artifacts(run)
        .map_err(|err| BusError::Store(err.to_string()))?
    {
        let (record, _) = store
            .read_artifact(&artifact)
            .map_err(|err| BusError::Store(err.to_string()))?;
        if record.kind == kind {
            return Ok(Some(artifact.to_string()));
        }
    }
    Ok(None)
}

fn explain_stored_proof(
    store: &Store,
    id: &ProofId,
    requested_id: &str,
) -> Result<Option<ExplainReply>, BusError> {
    let Some(proof) = store
        .get_proof(id)
        .map_err(|err| BusError::Store(err.to_string()))?
    else {
        return Ok(None);
    };
    let artifacts = store
        .proof_artifacts(&proof.id)
        .map_err(|err| BusError::Store(err.to_string()))?;
    let mut provenance = vec![
        format!("revision {}", proof.revision),
        format!("oracle seal {}", proof.oracle_seal),
    ];
    let mut revision_range_seen = false;
    for artifact in &artifacts {
        let (record, bytes) = store
            .read_artifact(artifact)
            .map_err(|err| BusError::Store(err.to_string()))?;
        if record.kind == "revision-range" {
            if revision_range_seen {
                return Err(BusError::Store(
                    "proof has more than one revision-range artifact".into(),
                ));
            }
            revision_range_seen = true;
            let range = parse_revision_range_evidence(&bytes)?;
            provenance.push(format!("base {} ({})", range.base_commit, range.base_ref));
            provenance.push(format!("head {} ({})", range.head_commit, range.head_ref));
            provenance.push(format!("merge base {}", range.merge_base));
        }
        provenance.push(format!("evidence {artifact}"));
    }
    Ok(Some(ExplainReply {
        id: requested_id.to_owned(),
        kind: "proof".into(),
        summary: format!(
            "proof {} is {} for obligation {}",
            proof.id, proof.verdict, proof.obligation
        ),
        provenance,
    }))
}

/// Exact base/head range the run measured, when it recorded one.
fn stored_range(store: &Store, run: &RunId) -> Option<RevisionRange> {
    for artifact in store.run_artifacts(run).ok()? {
        let (record, bytes) = store.read_artifact(&artifact).ok()?;
        if record.kind == "revision-range" {
            return parse_revision_range_evidence(&bytes).ok();
        }
    }
    None
}

/// The single protection snapshot of `kind` attached to `run`, if any.
fn snapshot_artifact(
    store: &Store,
    run: &RunId,
    kind: &str,
) -> Result<Option<ProtectionSnapshot>, BusError> {
    let mut found = None;
    for artifact in store
        .run_artifacts(run)
        .map_err(|err| BusError::Store(err.to_string()))?
    {
        let (record, bytes) = store
            .read_artifact(&artifact)
            .map_err(|err| BusError::Store(err.to_string()))?;
        if record.kind != kind {
            continue;
        }
        if found.is_some() {
            return Err(BusError::Store(format!(
                "run {run} has more than one {kind}"
            )));
        }
        found = Some(
            serde_json::from_slice(&bytes)
                .map_err(|err| BusError::Store(format!("invalid {kind} on run {run}: {err}")))?,
        );
    }
    Ok(found)
}

fn stored_oracle_replacement(
    store: &Store,
    run: &RunId,
) -> Result<Option<(OracleReplacementDocument, OracleReplacementReview)>, BusError> {
    let mut found = None;
    for artifact in store
        .run_artifacts(run)
        .map_err(|err| BusError::Store(err.to_string()))?
    {
        let (record, bytes) = store
            .read_artifact(&artifact)
            .map_err(|err| BusError::Store(err.to_string()))?;
        if record.kind != ORACLE_REPLACEMENT_KIND {
            continue;
        }
        if found.is_some() {
            return Err(BusError::Store(format!(
                "run {run} has more than one OracleSeal replacement proposal"
            )));
        }
        let document: OracleReplacementDocument =
            serde_json::from_slice(&bytes).map_err(|err| {
                BusError::Store(format!(
                    "invalid OracleSeal replacement proposal on run {run}: {err}"
                ))
            })?;
        if document.schema_v != 1 {
            return Err(BusError::Store(format!(
                "unknown OracleSeal replacement schema {} on run {run}",
                document.schema_v
            )));
        }
        let digest = record.content_hash.to_string();
        let subject = format!("oracle-replacement-{}", &digest[..16]);
        let approval_decision = store
            .human_decisions_for_subject(&subject)
            .map_err(|err| BusError::Store(err.to_string()))?
            .into_iter()
            .find(|decision| {
                matches!(decision.role.as_str(), "qa" | "product")
                    && decision.decision == "accept_as_intended"
                    && decision.artifact_digest == digest
            })
            .map(|decision| decision.id);
        let review = OracleReplacementReview {
            subject,
            artifact_digest: digest,
            change: document.change.clone(),
            base_revision: document.base_revision.clone(),
            head_revision: document.head_revision.clone(),
            head_content_revision: document.head_content_revision.clone(),
            merge_base: document.merge_base.clone(),
            base_seal: document.base_seal.clone(),
            base_seal_digest: document.base_seal_digest.clone(),
            head_seal: document.head_seal.clone(),
            head_seal_digest: document.head_seal_digest.clone(),
            changed_obligations: document.changed_obligations.clone(),
            obligation_replacements: document.obligation_replacements.clone(),
            approved: approval_decision.is_some(),
            approval_decision,
        };
        found = Some((document, review));
    }
    Ok(found)
}

/// Fold measured protection deltas and their gate findings into one axis.
///
/// A lost critical branch stays visible whatever the rest of the summary says:
/// `lost_critical_branches` is filled from the deltas themselves, so nine
/// improved flows cannot empty it.
fn protection_axis_from(
    deltas: &[ProtectionDelta],
    findings: &[ProtectionFinding],
) -> ProtectionAxis {
    let mut lost_flows = Vec::new();
    let mut lost_critical_branches = Vec::new();
    for delta in deltas {
        if matches!(delta.state, ProtectionDeltaState::Lost) {
            lost_flows.push(delta.flow.clone());
        }
        lost_critical_branches.extend(delta.lost_critical_branches.iter().cloned());
    }
    lost_flows.sort();
    lost_flows.dedup();
    lost_critical_branches.sort();
    lost_critical_branches.dedup();
    let (blocking_findings, warning_findings): (Vec<_>, Vec<_>) = findings
        .iter()
        .filter(|finding| finding.severity != Severity::Info)
        .cloned()
        .partition(|finding| finding.severity == Severity::Error);
    let state = if !blocking_findings.is_empty() || !lost_critical_branches.is_empty() {
        AxisState::Blocking
    } else if warning_findings.is_empty() {
        AxisState::Clean
    } else {
        AxisState::Warnings
    };
    ProtectionAxis {
        state,
        measured: true,
        summary: summarise(deltas),
        lost_flows,
        lost_critical_branches,
        blocking_findings,
        warning_findings,
    }
}

/// Project the debt ratchet onto the verdict axis.
///
/// Existing debt is counted and never blocks adoption; only findings this
/// change introduced or brought back are classified by rule family.
fn debt_axis_from(reply: &DebtReply) -> DebtAxis {
    let mut new = Vec::new();
    let mut returned = Vec::new();
    for summary in &reply.findings {
        let Some((bucket, rest)) = summary.split_once(": ") else {
            continue;
        };
        let (id, rule) = match rest.split_once(" (") {
            Some((id, rule)) => (id, rule.trim_end_matches(')')),
            None => (rest, ""),
        };
        let item = DebtItem {
            id: id.to_owned(),
            rule: rule.to_owned(),
            blocking: debt_rule_blocks(rule),
        };
        match bucket {
            "new" => new.push(item),
            "returned" => returned.push(item),
            _ => {}
        }
    }
    new.sort_by(|left, right| left.id.cmp(&right.id));
    returned.sort_by(|left, right| left.id.cmp(&right.id));
    let blocking = new.iter().chain(&returned).any(|item| item.blocking);
    let state = if !reply.comparison_present {
        AxisState::Unmeasured
    } else if blocking {
        AxisState::Blocking
    } else if new.is_empty() && returned.is_empty() {
        AxisState::Clean
    } else {
        AxisState::Warnings
    };
    DebtAxis {
        state,
        comparison_present: reply.comparison_present,
        existing: reply.existing,
        fixed: reply.fixed,
        excepted: reply.excepted,
        new,
        returned,
    }
}

/// Test stability from the run's persisted analytics.
///
/// A mandatory flake is only escalated when deterministic triage could not
/// classify a *first* occurrence of a failure on a test bound to a high or
/// critical obligation. A known, already-clustered flake stays a warning.
fn stability_axis(
    repo: &Path,
    store: &Store,
    run: &StoredRun,
    compiled: &Compiled,
) -> (StabilityAxis, Vec<Limitation>) {
    let Ok(analytics) = read_single_run_json(store, &run.id, "test-analytics") else {
        return (StabilityAxis::default(), Vec::new());
    };
    let flaky = values_at(&analytics, "/flaky_tests");
    let occurrences = values_at(&analytics, "/failure_occurrences");
    let mandatory_paths = mandatory_test_paths(repo, compiled);
    let mut unresolved = Vec::new();
    let mut unknown_failures = 0_u64;
    for occurrence in occurrences {
        if occurrence.get("classification").and_then(Value::as_str) != Some("unknown") {
            continue;
        }
        unknown_failures = unknown_failures.saturating_add(1);
        let first_seen = occurrence
            .get("previous_occurrences")
            .and_then(Value::as_u64)
            .is_some_and(|count| count == 0);
        let suite = occurrence
            .get("suite")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let name = occurrence
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if first_seen && mandatory_paths.contains(&normalize_path(suite)) {
            unresolved.push(format!("{suite}::{name}"));
        }
    }
    unresolved.sort();
    unresolved.dedup();
    let flaky_count = u64::try_from(flaky.len()).unwrap_or(u64::MAX);
    let state = if !unresolved.is_empty() {
        AxisState::Blocking
    } else if flaky_count > 0 || unknown_failures > 0 {
        AxisState::Warnings
    } else {
        AxisState::Clean
    };
    (
        StabilityAxis {
            state,
            measured: true,
            flaky: flaky_count,
            unknown_failures,
            unresolved_mandatory_flakes: unresolved,
        },
        Vec::new(),
    )
}

/// Deterministic UI integrity from the run's persisted ratchet.
///
/// The collector is the only thing that knows whether a route/state/viewport
/// was reachable, so `unmeasured` is reported by the producer rather than
/// guessed here. With no artifact at all this change has no UI surface and the
/// axis is `not_applicable`.
fn ui_integrity_axis(
    store: &Store,
    run: &StoredRun,
    _compiled: &Compiled,
) -> Result<(UiIntegrityAxis, Vec<Limitation>), BusError> {
    let Ok(document) = read_single_run_json(store, &run.id, UI_INTEGRITY_DELTA_KIND) else {
        return Ok((UiIntegrityAxis::default(), Vec::new()));
    };
    if document.get("schema_v").and_then(Value::as_u64) != Some(1) {
        return Err(BusError::Store(
            "unknown ui-integrity-delta schema version".into(),
        ));
    }
    let axis = UiIntegrityAxis {
        state: parse_axis_state(document.get("state").and_then(Value::as_str))?,
        new: parse_ui_findings(&document, "new")?,
        returned: parse_ui_findings(&document, "returned")?,
        existing: json_u64(&document, "existing"),
        fixed: json_u64(&document, "fixed"),
        excepted: json_u64(&document, "excepted"),
        unmeasured_states: values_at(&document, "/unmeasured_states")
            .iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect(),
        truncated: document
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || document
                .get("responsive_truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false),
    };
    Ok((axis, Vec::new()))
}

fn delta_triangle_axis(
    store: &Store,
    run: &StoredRun,
) -> Result<(DeltaTriangleAxis, Vec<Limitation>), BusError> {
    let Ok(document) = read_single_run_json(store, &run.id, DELTA_TRIANGLE_KIND) else {
        return Ok((DeltaTriangleAxis::default(), Vec::new()));
    };
    if document.get("schema_v").and_then(Value::as_u64) != Some(1) {
        return Err(BusError::Store(
            "unknown delta-triangle schema version".into(),
        ));
    }
    let unmeasured_programs = values_at(&document, "/unmeasured_programs")
        .iter()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect::<Vec<_>>();
    let mut findings = Vec::new();
    for value in values_at(&document, "/findings") {
        let severity = match value.get("severity").and_then(Value::as_str) {
            Some("info") => Severity::Info,
            Some("warn") => Severity::Warn,
            Some("error") => Severity::Error,
            other => {
                return Err(BusError::Store(format!(
                    "delta-triangle finding has unknown severity `{}`",
                    other.unwrap_or("<missing>")
                )));
            }
        };
        findings.push(DeltaFindingRef {
            check: json_string(value, "check")?,
            severity,
            program: json_string(value, "program")?,
            detail: json_string(value, "detail")?,
        });
    }
    let axis = DeltaTriangleAxis {
        state: parse_axis_state(document.get("state").and_then(Value::as_str))?,
        spec_changed: document
            .get("spec_changed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        code_changed: document
            .get("code_changed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        behavior_changed: document
            .get("behavior_changed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        measured_programs: json_u64(&document, "measured_programs"),
        changed_programs: values_at(&document, "/changed_programs")
            .iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect(),
        readings: values_at(&document, "/readings")
            .iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect(),
        findings,
        unmeasured_programs: unmeasured_programs.clone(),
    };
    let replay_detail = document
        .get("replay_limitation")
        .and_then(Value::as_str)
        .filter(|detail| !detail.is_empty());
    let limitations = (!unmeasured_programs.is_empty())
        .then(|| Limitation {
            axis: "delta_triangle".into(),
            detail: format!(
                "same-program base/head replay was incomplete for {}{}",
                unmeasured_programs.join(", "),
                replay_detail.map_or_else(String::new, |detail| format!(": {detail}"))
            ),
        })
        .into_iter()
        .collect();
    Ok((axis, limitations))
}

fn parse_axis_state(token: Option<&str>) -> Result<AxisState, BusError> {
    match token {
        Some("not_applicable") => Ok(AxisState::NotApplicable),
        Some("clean") => Ok(AxisState::Clean),
        Some("warnings") => Ok(AxisState::Warnings),
        Some("blocking") => Ok(AxisState::Blocking),
        Some("unmeasured") => Ok(AxisState::Unmeasured),
        other => Err(BusError::Store(format!(
            "unknown ui-integrity axis state `{}`",
            other.unwrap_or("<missing>")
        ))),
    }
}

fn parse_ui_findings(document: &Value, field: &str) -> Result<Vec<UiFindingRef>, BusError> {
    let mut out = Vec::new();
    for value in values_at(document, &format!("/{field}")) {
        let severity = match value.get("severity").and_then(Value::as_str) {
            Some("info") => Severity::Info,
            Some("warn") => Severity::Warn,
            Some("error") => Severity::Error,
            other => {
                return Err(BusError::Store(format!(
                    "ui-integrity finding has unknown severity `{}`",
                    other.unwrap_or("<missing>")
                )));
            }
        };
        out.push(UiFindingRef {
            check: json_string(value, "check")?,
            severity,
            subject: json_string(value, "subject")?,
            route: json_string(value, "route")?,
            viewport: json_string(value, "viewport")?,
            detail: json_string(value, "detail")?,
        });
    }
    Ok(out)
}

fn json_string(value: &Value, field: &str) -> Result<String, BusError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| BusError::Store(format!("ui-integrity finding omitted {field}")))
}

fn json_u64(value: &Value, field: &str) -> u64 {
    value.get(field).and_then(Value::as_u64).unwrap_or_default()
}

/// Repository test paths bound to a high or critical obligation.
fn mandatory_test_paths(repo: &Path, compiled: &Compiled) -> BTreeSet<String> {
    let mandatory = compiled
        .obligations
        .iter()
        .filter(|item| matches!(item.risk, RiskLevel::High | RiskLevel::Critical))
        .map(|item| item.id.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    if mandatory.is_empty() {
        return BTreeSet::new();
    }
    load_test_bindings(repo)
        .unwrap_or_default()
        .into_iter()
        .filter(|binding| {
            binding
                .obligations
                .iter()
                .any(|obligation| mandatory.contains(obligation))
        })
        .map(|binding| binding.path)
        .collect()
}

/// Join the backward-compatible `ProofVerdict` token with the composite
/// change-level verdict. `blocking` and the exit code follow the composite
/// state, so a lost protection net or a new UI regression fails CI even when
/// every sealed obligation is `PROVEN`.
fn combine_verify(
    change: &str,
    proofs: Vec<ProofSummary>,
    verdicts: &[ProofVerdict],
    quality: ChangeQualityVerdict,
) -> VerifyReply {
    let combined = combine_verdicts(verdicts);
    VerifyReply {
        change: change.to_owned(),
        verdict: combined.as_str().to_owned(),
        blocking: quality.blocking(),
        proofs,
        state: quality.state.as_str().to_owned(),
        quality,
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
        let runner = yaml_optional_binding_string(binding, "runner", &path, index)?;
        if let Some(runner) = runner.as_deref()
            && !matches!(
                runner,
                "cargo-test"
                    | "vitest"
                    | "storybook-vitest"
                    | "storybook-vitest-v8"
                    | "jest"
                    | "bun-test"
                    | "go-test"
                    | "playwright"
                    | "npm-test"
            )
        {
            return Err(BusError::Runtime(format!(
                "quality policy {} test binding {} has unknown runner {runner}",
                path.display(),
                index + 1
            )));
        }
        let suite = yaml_optional_binding_string(binding, "suite", &path, index)?
            .map(|suite| normalize_path(&suite));
        let case = yaml_optional_binding_string(binding, "case", &path, index)?;
        if suite.is_some() && case.is_none() {
            return Err(BusError::Runtime(format!(
                "quality policy {} test binding {} cannot name suite without case",
                path.display(),
                index + 1
            )));
        }
        if case.is_some() && runner.is_none() {
            return Err(BusError::Runtime(format!(
                "quality policy {} test binding {} requires runner with case",
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
            runner,
            suite,
            case,
            obligations,
            cost,
            flake_penalty,
        });
    }
    Ok(out)
}

/// Load and validate `ui_integrity` from `.weavatrix-quality/config.yaml`.
///
/// A repository with no section gets the disabled default, which makes the axis
/// `not_applicable`. A section that is present but invalid fails the run: a
/// typo in an allowance must never quietly widen what is accepted.
fn load_ui_integrity_policy(repo: &Path) -> Result<UiIntegrityPolicy, BusError> {
    let path = repo.join(".weavatrix-quality").join("config.yaml");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(UiIntegrityPolicy::default());
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
    let Some(section) = yaml_get(root, "ui_integrity") else {
        return Ok(UiIntegrityPolicy::default());
    };
    parse_ui_policy(section, &utc_date())
        .map_err(|err| BusError::Runtime(format!("{}: {err}", path.display())))
}

/// Turn the analysis policy into browser collection bounds.
///
/// Every semantic target a sealed predicate names is passed through as a
/// required test id, so the collector can never drop the exact node an
/// obligation depends on to stay under its node ceiling.
fn ui_collection_config(
    policy: &UiIntegrityPolicy,
    oracles: &[ProgramOracle],
) -> Option<UiCollectionConfig> {
    if !policy.enabled {
        return None;
    }
    let mut required = BTreeSet::new();
    for oracle in oracles {
        collect_predicate_test_ids(&oracle.expected, &mut required);
        if let Some(condition) = &oracle.condition {
            collect_predicate_test_ids(condition, &mut required);
        }
    }
    Some(UiCollectionConfig {
        enabled: true,
        max_nodes: policy.max_nodes,
        geometry_tolerance_px: policy.geometry_tolerance_px,
        settle_timeout_ms: 2_000,
        test_id_attribute: "data-testid".into(),
        required_test_ids: required.into_iter().collect(),
        responsive_breakpoints: policy.responsive.enabled,
    })
}

/// Every `test_id` any nested predicate target names.
fn collect_predicate_test_ids(predicate: &Value, out: &mut BTreeSet<String>) {
    match predicate {
        Value::Object(map) => {
            for (key, value) in map {
                if key == "test_id"
                    && let Some(id) = value.as_str().filter(|id| !id.is_empty())
                {
                    out.insert(id.to_owned());
                }
                collect_predicate_test_ids(value, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_predicate_test_ids(item, out);
            }
        }
        _ => {}
    }
}

fn load_browser_policy(
    repo: &Path,
    obligations: &[TestObligation],
) -> Result<Option<BrowserPolicy>, BusError> {
    load_browser_policy_with(repo, obligations, None)
}

/// Load a browser policy, optionally supplying the Playwright installation.
///
/// A base-revision worktree is a fresh checkout, so it has no `node_modules`:
/// the browser engine is toolchain, not source, and is deliberately not
/// versioned. Replaying base therefore reuses the working repository's engine.
/// That is also the only correct comparison — measuring base with a different
/// browser build would confound the very geometry the ratchet compares.
fn load_browser_policy_with(
    repo: &Path,
    obligations: &[TestObligation],
    module_root: Option<&Path>,
) -> Result<Option<BrowserPolicy>, BusError> {
    let Some(mut policy) = load_browser_runtime_with(repo, module_root)? else {
        return Ok(None);
    };
    let path = repo.join(".weavatrix-quality").join("config.yaml");
    let raw = std::fs::read_to_string(&path).map_err(|err| {
        BusError::Runtime(format!(
            "cannot read quality policy {}: {err}",
            path.display()
        ))
    })?;
    let value: serde_yaml::Value = serde_yaml::from_str(&raw).map_err(|err| {
        BusError::Runtime(format!("invalid quality policy {}: {err}", path.display()))
    })?;
    let root = value.as_mapping().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} must be a mapping",
            path.display()
        ))
    })?;
    let browser = yaml_get(root, "browser")
        .and_then(serde_yaml::Value::as_mapping)
        .ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} browser must be a mapping",
                path.display()
            ))
        })?;
    policy.programs = parse_browser_programs(repo, &path, browser, obligations)?;
    Ok(Some(policy))
}

/// Load only the versioned browser runtime coordinates. Differential replay
/// intentionally supplies the exact head `TestProgram` to both sides, so a
/// stale or absent base program file must not replace it.
fn load_browser_runtime_with(
    repo: &Path,
    module_root: Option<&Path>,
) -> Result<Option<BrowserPolicy>, BusError> {
    let path = repo.join(".weavatrix-quality").join("config.yaml");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
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
    let Some(browser) = yaml_get(root, "browser") else {
        return Ok(None);
    };
    let browser = browser.as_mapping().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} browser must be a mapping",
            path.display()
        ))
    })?;
    parse_browser_runtime(repo, &path, browser, module_root).map(Some)
}

fn load_live_browser_policy(
    repo: &Path,
    compiled: &Compiled,
    store: &Store,
) -> Result<Option<BrowserPolicy>, BusError> {
    let Some(mut policy) = load_browser_policy(repo, &compiled.obligations)? else {
        return Ok(None);
    };
    let stored = store
        .latest_program_revisions_for_change(&compiled.change)
        .map_err(|err| BusError::Store(err.to_string()))?;
    if stored.len() > 500 {
        return Err(BusError::Store(
            "more than 500 promoted browser programs require explicit repository curation".into(),
        ));
    }
    let mut ids = policy
        .programs
        .iter()
        .map(|configured| configured.program.id.to_string())
        .collect::<BTreeSet<_>>();
    for (record, body) in stored {
        let candidate: Value = serde_json::from_slice(&body).map_err(|err| {
            BusError::Store(format!(
                "stored TestProgram {} revision {} is malformed: {err}",
                record.program, record.revision
            ))
        })?;
        let validated = validate_author_candidate(repo, compiled, &candidate)?;
        if validated.program.id.as_str() != record.program {
            return Err(BusError::Store(format!(
                "stored TestProgram {} revision {} has a different body id {}",
                record.program, record.revision, validated.program.id
            )));
        }
        if validated.seal_id != record.seal {
            continue;
        }
        if !ids.insert(record.program.clone()) {
            return Err(BusError::Store(format!(
                "browser TestProgram {} is configured both as a repository file and a promoted revision",
                record.program
            )));
        }
        policy.programs.push(ConfiguredBrowserProgram {
            path: format!("wvq-program:{}@{}", record.program, record.revision),
            program: validated.program,
            oracles: validated.oracles,
        });
    }
    Ok(Some(policy))
}

fn parse_browser_runtime(
    repo: &Path,
    path: &Path,
    browser: &serde_yaml::Mapping,
    module_root_override: Option<&Path>,
) -> Result<BrowserPolicy, BusError> {
    let allowed = [
        "base_url",
        "engine",
        "headless",
        "timeout_ms",
        "module_root",
        "programs",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if let Some(unknown) = browser
        .keys()
        .filter_map(serde_yaml::Value::as_str)
        .find(|key| !allowed.contains(key))
    {
        return Err(BusError::Runtime(format!(
            "quality policy {} browser has unknown field {unknown}",
            path.display()
        )));
    }
    let base_url = yaml_required_runtime_string(browser, "base_url", path)?;
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err(BusError::Runtime(format!(
            "quality policy {} browser.base_url must use http or https",
            path.display()
        )));
    }
    let engine = yaml_get(browser, "engine")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("chromium")
        .to_owned();
    if !matches!(engine.as_str(), "chromium" | "firefox" | "webkit") {
        return Err(BusError::Runtime(format!(
            "quality policy {} has unknown browser engine {engine}",
            path.display()
        )));
    }
    let headless = yaml_get(browser, "headless").map_or(Ok(true), |value| {
        value.as_bool().ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} browser.headless must be boolean",
                path.display()
            ))
        })
    })?;
    let timeout_ms = yaml_get(browser, "timeout_ms").map_or(Ok(30_000), |value| {
        value
            .as_u64()
            .filter(|timeout| (1..=120_000).contains(timeout))
            .ok_or_else(|| {
                BusError::Runtime(format!(
                    "quality policy {} browser.timeout_ms must be between 1 and 120000",
                    path.display()
                ))
            })
    })?;
    let module_root = if let Some(override_root) = module_root_override {
        override_root.to_path_buf()
    } else {
        let module_root_raw = yaml_get(browser, "module_root")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or(".");
        checked_repo_path(repo, module_root_raw, "browser.module_root")?.1
    };
    if !module_root.join("package.json").is_file() {
        return Err(BusError::Runtime(format!(
            "quality policy {} browser.module_root has no package.json: {}",
            path.display(),
            module_root.display()
        )));
    }
    Ok(BrowserPolicy {
        base_url,
        browser: engine,
        headless,
        timeout: Duration::from_millis(timeout_ms),
        module_root,
        programs: Vec::new(),
    })
}

fn parse_browser_programs(
    repo: &Path,
    path: &Path,
    browser: &serde_yaml::Mapping,
    obligations: &[TestObligation],
) -> Result<Vec<ConfiguredBrowserProgram>, BusError> {
    let Some(programs_value) = yaml_get(browser, "programs") else {
        return Ok(Vec::new());
    };
    let programs = programs_value.as_sequence().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} browser.programs must be a list",
            path.display()
        ))
    })?;
    let known = obligations
        .iter()
        .map(|obligation| (obligation.id.as_str(), obligation))
        .collect::<BTreeMap<_, _>>();
    let mut seen_paths = BTreeSet::new();
    let mut seen_ids = BTreeSet::new();
    let mut configured = Vec::new();
    for (index, item) in programs.iter().enumerate() {
        let raw_path = item
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                BusError::Runtime(format!(
                    "quality policy {} browser program {} must be a path string",
                    path.display(),
                    index + 1
                ))
            })?;
        let (program_path, absolute) = checked_repo_path(repo, raw_path, "browser program path")?;
        if !seen_paths.insert(program_path.clone()) {
            return Err(BusError::Runtime(format!(
                "quality policy {} repeats browser program {program_path}",
                path.display()
            )));
        }
        let raw = std::fs::read_to_string(&absolute).map_err(|err| {
            BusError::Runtime(format!(
                "cannot read browser TestProgram {}: {err}",
                absolute.display()
            ))
        })?;
        let program = TestProgram::from_json(&raw)
            .map_err(|err| BusError::Runtime(format!("{}: {err}", absolute.display())))?;
        if !seen_ids.insert(program.id.to_string()) {
            return Err(BusError::Runtime(format!(
                "duplicate browser TestProgram id {}",
                program.id
            )));
        }
        let mut oracles = Vec::new();
        for obligation in &program.obligations {
            let sealed = known.get(obligation.as_str()).ok_or_else(|| {
                BusError::Runtime(format!(
                    "browser TestProgram {} names unknown obligation {obligation}",
                    program.id
                ))
            })?;
            let expected = sealed.expected.as_ref().ok_or_else(|| {
                BusError::Runtime(format!(
                    "browser TestProgram {} cannot assert {obligation}: quality.yaml has no sealed expected predicate",
                    program.id
                ))
            })?;
            oracles.push(ProgramOracle {
                obligation: obligation.clone(),
                condition: sealed
                    .condition
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
                expected: serde_json::to_value(expected)
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
            });
        }
        configured.push(ConfiguredBrowserProgram {
            path: program_path,
            program,
            oracles,
        });
    }
    Ok(configured)
}

fn browser_test_bindings(policy: &BrowserPolicy) -> Vec<TestBinding> {
    policy
        .programs
        .iter()
        .map(|configured| TestBinding {
            path: configured.path.clone(),
            runner: Some("playwright-browser".into()),
            suite: Some(configured.path.clone()),
            case: Some(configured.program.id.to_string()),
            obligations: configured
                .program
                .obligations
                .iter()
                .map(ToString::to_string)
                .collect(),
            cost: 500,
            flake_penalty: 0,
        })
        .collect()
}

fn checked_repo_path(repo: &Path, raw: &str, label: &str) -> Result<(String, PathBuf), BusError> {
    let normalized = normalize_path(raw);
    let path = Path::new(&normalized);
    if normalized.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(BusError::Runtime(format!(
            "{label} must stay repository-relative"
        )));
    }
    Ok((normalized.clone(), repo.join(normalized)))
}

fn yaml_required_runtime_string(
    mapping: &serde_yaml::Mapping,
    key: &str,
    path: &Path,
) -> Result<String, BusError> {
    yaml_get(mapping, key)
        .and_then(serde_yaml::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} browser.{key} must be non-empty",
                path.display()
            ))
        })
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

fn yaml_optional_binding_string(
    mapping: &serde_yaml::Mapping,
    key: &str,
    path: &Path,
    index: usize,
) -> Result<Option<String>, BusError> {
    yaml_get(mapping, key).map_or(Ok(None), |value| {
        value
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| {
                BusError::Runtime(format!(
                    "quality policy {} test binding {} requires non-empty {key}",
                    path.display(),
                    index + 1
                ))
            })
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

fn historical_selection_candidates(
    store: &Store,
    impact: &wvq_intelligence::ImpactedSurface,
) -> Result<Vec<HistoricalTestCandidate>, BusError> {
    let mut candidates = store
        .historical_tests_for_nodes(&impact.all_nodes(), 2, 100_000)
        .map_err(|err| BusError::Store(err.to_string()))?;
    candidates.sort_by(|left, right| {
        right
            .defensive_misses
            .cmp(&left.defensive_misses)
            .then_with(|| right.matched_nodes.len().cmp(&left.matched_nodes.len()))
            .then_with(|| right.minimum_observations.cmp(&left.minimum_observations))
            .then_with(|| left.test_path.cmp(&right.test_path))
    });
    candidates.truncate(500);
    Ok(candidates)
}

fn merge_historical_selection(
    repo: &Path,
    historical: &[HistoricalTestCandidate],
    selected: &mut Vec<String>,
    explanations: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for candidate in historical.iter().filter(|candidate| {
        repo.join(&candidate.test_path).is_file() && is_test_path(&candidate.test_path)
    }) {
        let path = normalize_path(&candidate.test_path);
        let reasons = explanations.entry(path.clone()).or_default();
        if candidate.minimum_observations > 0 {
            reasons.insert(format!(
                "selected by repeated measured coverage of {} impacted graph node(s), minimum {} observations, evidence revision {}",
                candidate.matched_nodes.len(),
                candidate.minimum_observations,
                candidate.last_revision
            ));
        }
        if candidate.defensive_misses > 0 {
            reasons.insert(format!(
                "selected after {} defensive full-run miss(es) across {} impacted graph node(s), evidence revision {}",
                candidate.defensive_misses,
                candidate.matched_nodes.len(),
                candidate.last_revision
            ));
        }
        selected.push(path);
    }
}

fn merge_impacted_stories(
    repo: &Path,
    impact: &wvq_intelligence::ImpactedSurface,
    selected: &mut Vec<String>,
    explanations: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for path in impact
        .all_nodes()
        .iter()
        .filter_map(|node| test_path_from_node_id(node))
        .filter(|path| is_story_path(path) && repo.join(path).is_file())
    {
        explanations
            .entry(path.clone())
            .or_default()
            .insert("selected as a Storybook state in the base/head Weavatrix impact union".into());
        selected.push(path);
    }
}

fn build_live_selection(
    repo: &Path,
    static_report: &Value,
    diff: &Value,
    impact: &wvq_intelligence::ImpactedSurface,
    obligations: &[ObligationNeed],
    additional_bindings: &[TestBinding],
    historical: &[HistoricalTestCandidate],
) -> Result<LiveSelection, BusError> {
    let (mut static_selected, static_explanations) = static_and_base_tests(static_report, diff);
    let mut explanations = static_selected
        .iter()
        .cloned()
        .zip(static_explanations)
        .map(|(path, reasons)| (path, reasons.into_iter().collect::<BTreeSet<_>>()))
        .collect::<BTreeMap<_, _>>();
    merge_historical_selection(repo, historical, &mut static_selected, &mut explanations);
    merge_impacted_stories(repo, impact, &mut static_selected, &mut explanations);
    let known = obligations
        .iter()
        .map(|obligation| obligation.id.clone())
        .collect::<BTreeSet<_>>();
    let bindings = merged_test_bindings(repo, &known, additional_bindings)?;
    let candidates = selection_candidates(repo, &bindings);
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
        .filter(|binding| {
            binding.case.is_some()
                && selected.contains(&binding.path)
                && repo.join(&binding.path).is_file()
        })
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

fn merged_test_bindings(
    repo: &Path,
    known: &BTreeSet<String>,
    additional_bindings: &[TestBinding],
) -> Result<Vec<TestBinding>, BusError> {
    let mut merged =
        BTreeMap::<(String, Option<String>, Option<String>, Option<String>), TestBinding>::new();
    let mut configured_bindings = load_test_bindings(repo)?;
    configured_bindings.extend(additional_bindings.iter().cloned());
    for binding in configured_bindings {
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
        let key = (
            binding.path.clone(),
            binding.runner.clone(),
            binding.suite.clone(),
            binding.case.clone(),
        );
        let entry = merged.entry(key).or_insert_with(|| TestBinding {
            path: binding.path.clone(),
            runner: binding.runner.clone(),
            suite: binding.suite.clone(),
            case: binding.case.clone(),
            obligations: BTreeSet::new(),
            cost: binding.cost,
            flake_penalty: binding.flake_penalty,
        });
        entry.obligations.extend(binding.obligations);
        entry.cost = entry.cost.min(binding.cost);
        entry.flake_penalty = entry.flake_penalty.max(binding.flake_penalty);
    }
    Ok(merged.into_values().collect())
}

fn selection_candidates(repo: &Path, bindings: &[TestBinding]) -> Vec<TestCandidate> {
    let mut candidates = BTreeMap::<String, TestCandidate>::new();
    for binding in bindings
        .iter()
        .filter(|binding| binding.case.is_some() && repo.join(&binding.path).is_file())
    {
        let entry = candidates
            .entry(binding.path.clone())
            .or_insert_with(|| TestCandidate {
                id: binding.path.clone(),
                cost: binding.cost,
                flake_penalty: binding.flake_penalty,
                covers: BTreeSet::new(),
                explanation: Vec::new(),
            });
        entry.cost = entry.cost.min(binding.cost);
        entry.flake_penalty = entry.flake_penalty.max(binding.flake_penalty);
        entry.covers.extend(binding.obligations.iter().cloned());
    }
    for candidate in candidates.values_mut() {
        candidate.explanation.push(format!(
            "quality policy binds exact test case evidence to: {}",
            candidate
                .covers
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    candidates.into_values().collect()
}

fn live_selection_report(selection: &LiveSelection, historical_candidates: usize) -> Value {
    let selected = selection
        .selected
        .iter()
        .zip(&selection.explanations)
        .map(|(path, explanation)| {
            json!({
                "path": path,
                "explanation": explanation,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema_v": 2,
        "algorithm": "weavatrix-base-head-history-union+greedy-weighted-set-cover",
        "selected": selected,
        "historical_candidates": historical_candidates,
        "minimum_history_observations": 2,
        "uncovered_mandatory": selection.uncovered_mandatory,
        "uncovered_obligations": selection.uncovered_all,
    })
}

struct SelectionAuditArtifactInput<'a> {
    missed: &'a [StoredTestCaseIdentity],
    learned_paths: &'a BTreeSet<String>,
    impact_nodes_total: usize,
    impact_nodes_considered: usize,
    learning_truncated: bool,
}

fn audit_live_selection(
    repo: &Path,
    store: &Store,
    impacted_raw: &str,
    full_raw: &str,
) -> Result<SelectionAuditReply, BusError> {
    let impacted_id =
        RunId::new(impacted_raw).map_err(|err| BusError::Identity(err.to_string()))?;
    let full_id = RunId::new(full_raw).map_err(|err| BusError::Identity(err.to_string()))?;
    if impacted_id == full_id {
        return Err(BusError::InvalidInput(
            "selection audit requires two distinct runs".into(),
        ));
    }
    if let Some(existing) = store
        .selection_audit_for_runs(&impacted_id, &full_id)
        .map_err(|err| BusError::Store(err.to_string()))?
    {
        return stored_selection_audit_reply(store, &existing);
    }
    let (impacted, _full) = load_shadow_runs(store, &impacted_id, &full_id)?;
    let impacted_summary = read_single_run_json(store, &impacted_id, "execution-summary")?;
    let full_summary = read_single_run_json(store, &full_id, "execution-summary")?;
    validate_shadow_scopes(&impacted_summary, &full_summary)?;
    let reduced = impacted_summary
        .get("effective_scope")
        .and_then(Value::as_str)
        == Some("impacted");
    let impacted_cases = store
        .test_case_results_for_run(&impacted_id)
        .map_err(|err| BusError::Store(err.to_string()))?;
    let full_cases = store
        .test_case_results_for_run(&full_id)
        .map_err(|err| BusError::Store(err.to_string()))?;
    let missed = if reduced {
        missed_failure_identities(&impacted_cases, &full_cases)
    } else {
        Vec::new()
    };
    let status = if !reduced {
        "not_reduced"
    } else if full_cases.is_empty() {
        "unmeasured"
    } else if missed.is_empty() {
        "corroborated"
    } else {
        "contradicted"
    };
    let impact = read_single_run_json(store, &impacted_id, "impacted-surface")?;
    let all_nodes = impact_nodes_from_artifact(&impact)?;
    let mut learned_paths = missed
        .iter()
        .filter_map(|case| resolve_observed_test_path(repo, &case.suite))
        .collect::<BTreeSet<_>>();
    let missed_count = u64::try_from(missed.len()).unwrap_or(u64::MAX);
    let all_node_count = all_nodes.len();
    let learning_truncated = learned_paths.len() > 500 || all_nodes.len() > 2_000;
    learned_paths = learned_paths.into_iter().take(500).collect();
    let learning_nodes = all_nodes.into_iter().take(2_000).collect::<Vec<_>>();
    let learned_count = u64::try_from(learned_paths.len()).unwrap_or(u64::MAX);
    let audit_id = format!(
        "selection-audit-{}",
        &sha256_hex(format!("{impacted_id}\0{full_id}").as_bytes())[..16]
    );
    let audit = StoredSelectionAudit {
        id: audit_id.clone(),
        impacted_run: impacted_id,
        full_run: full_id.clone(),
        change_id: impacted.change_id,
        revision: impacted.revision.clone(),
        status: status.into(),
        missed_failures: missed_count,
        learned_tests: learned_count,
    };
    store
        .put_selection_audit(&audit)
        .map_err(|err| BusError::Store(err.to_string()))?;
    for path in &learned_paths {
        store
            .observe_selection_miss(&audit_id, path, &learning_nodes, &impacted.revision)
            .map_err(|err| BusError::Store(err.to_string()))?;
    }
    let artifact = SelectionAuditArtifactInput {
        missed: &missed,
        learned_paths: &learned_paths,
        impact_nodes_total: all_node_count,
        impact_nodes_considered: learning_nodes.len(),
        learning_truncated,
    };
    let handle = persist_selection_audit_artifact(store, &full_id, &audit, &artifact)?;
    Ok(SelectionAuditReply {
        audit_id,
        status: status.into(),
        missed_failure_count: missed_count,
        learned_test_count: learned_count,
        evidence_handle: Some(handle),
    })
}

fn load_shadow_runs(
    store: &Store,
    impacted_id: &RunId,
    full_id: &RunId,
) -> Result<(StoredRun, StoredRun), BusError> {
    let impacted = store
        .get_run(impacted_id)
        .map_err(|err| BusError::Store(err.to_string()))?
        .ok_or_else(|| BusError::NotFound(format!("run {impacted_id}")))?;
    let full = store
        .get_run(full_id)
        .map_err(|err| BusError::Store(err.to_string()))?
        .ok_or_else(|| BusError::NotFound(format!("run {full_id}")))?;
    if impacted.change_id != full.change_id || impacted.revision != full.revision {
        return Err(BusError::Ambiguous(
            "selection audit runs do not share one change and revision".into(),
        ));
    }
    Ok((impacted, full))
}

fn persist_selection_audit_artifact(
    store: &Store,
    full_run: &RunId,
    audit: &StoredSelectionAudit,
    input: &SelectionAuditArtifactInput<'_>,
) -> Result<String, BusError> {
    let handle = format!("artifact-{}", audit.id);
    put_json_run_artifact(
        store,
        full_run,
        &handle,
        "selection-audit",
        &json!({
            "schema_v": 1,
            "audit_id": audit.id,
            "impacted_run": audit.impacted_run.as_str(),
            "full_run": audit.full_run.as_str(),
            "change": audit.change_id,
            "revision": audit.revision.as_str(),
            "status": audit.status,
            "missed_failure_count": audit.missed_failures,
            "missed_failures": input.missed.iter().take(500).map(|case| json!({
                "executor": case.executor,
                "suite": case.suite,
                "name": case.name,
                "status": case.status,
            })).collect::<Vec<_>>(),
            "learned_test_paths": input.learned_paths,
            "impact_nodes_total": input.impact_nodes_total,
            "impact_nodes_considered": input.impact_nodes_considered,
            "learning_truncated": input.learning_truncated,
            "runtime_llm_tokens": 0,
        }),
        &mut Vec::new(),
    )?;
    Ok(handle)
}

fn stored_selection_audit_reply(
    store: &Store,
    audit: &StoredSelectionAudit,
) -> Result<SelectionAuditReply, BusError> {
    let handle = format!("artifact-{}", audit.id);
    let artifact = ArtifactId::new(&handle).map_err(|err| BusError::Identity(err.to_string()))?;
    let present = store
        .get_artifact(&artifact)
        .map_err(|err| BusError::Store(err.to_string()))?
        .is_some();
    Ok(SelectionAuditReply {
        audit_id: audit.id.clone(),
        status: audit.status.clone(),
        missed_failure_count: audit.missed_failures,
        learned_test_count: audit.learned_tests,
        evidence_handle: present.then_some(handle),
    })
}

fn validate_shadow_scopes(impacted: &Value, full: &Value) -> Result<(), BusError> {
    let impacted_requested = impacted.get("requested_scope").and_then(Value::as_str);
    let full_requested = full.get("requested_scope").and_then(Value::as_str);
    let full_effective = full.get("effective_scope").and_then(Value::as_str);
    if impacted_requested != Some("impacted")
        || full_requested != Some("all")
        || full_effective != Some("all")
    {
        return Err(BusError::InvalidInput(
            "selection audit requires an impacted run followed by an effective full run".into(),
        ));
    }
    Ok(())
}

fn missed_failure_identities(
    impacted: &[StoredTestCaseIdentity],
    full: &[StoredTestCaseIdentity],
) -> Vec<StoredTestCaseIdentity> {
    let impacted_failures = impacted
        .iter()
        .filter(|case| matches!(case.status.as_str(), "fail" | "error"))
        .map(|case| (&case.executor, &case.suite, &case.name))
        .collect::<BTreeSet<_>>();
    full.iter()
        .filter(|case| matches!(case.status.as_str(), "fail" | "error"))
        .filter(|case| !impacted_failures.contains(&(&case.executor, &case.suite, &case.name)))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn impact_nodes_from_artifact(impact: &Value) -> Result<Vec<String>, BusError> {
    let mut nodes = BTreeSet::new();
    for field in ["base_only", "head_only", "shared", "removed_nodes"] {
        let values = impact
            .get(field)
            .and_then(Value::as_array)
            .ok_or_else(|| BusError::Store(format!("impacted-surface omitted {field}")))?;
        for value in values {
            let node = value.as_str().ok_or_else(|| {
                BusError::Store(format!("impacted-surface {field} contains a non-string"))
            })?;
            nodes.insert(node.to_owned());
        }
    }
    Ok(nodes.into_iter().collect())
}

fn resolve_observed_test_path(repo: &Path, suite: &str) -> Option<String> {
    let normalized = normalize_path(suite);
    if !is_test_path(&normalized) {
        return None;
    }
    let root = std::fs::canonicalize(repo).ok()?;
    let absolute = std::fs::canonicalize(repo.join(&normalized)).ok()?;
    let relative = absolute.strip_prefix(root).ok()?;
    absolute
        .is_file()
        .then(|| normalize_path(&relative.to_string_lossy()))
}

fn read_single_run_json(store: &Store, run: &RunId, kind: &str) -> Result<Value, BusError> {
    let mut found = None;
    for artifact in store
        .run_artifacts(run)
        .map_err(|err| BusError::Store(err.to_string()))?
    {
        let (record, bytes) = store
            .read_artifact(&artifact)
            .map_err(|err| BusError::Store(err.to_string()))?;
        if record.kind != kind {
            continue;
        }
        if found.is_some() {
            return Err(BusError::Store(format!(
                "run {run} has more than one {kind} artifact"
            )));
        }
        found =
            Some(serde_json::from_slice(&bytes).map_err(|err| {
                BusError::Store(format!("run {run} has malformed {kind}: {err}"))
            })?);
    }
    found.ok_or_else(|| BusError::Store(format!("run {run} has no {kind} artifact")))
}

fn build_execution_requests(
    repo: &Path,
    targets: &[ExecutorTarget],
    selection: &LiveSelection,
    browser_paths: &BTreeSet<String>,
    requested_scope: &str,
) -> (
    Vec<ExecutionRequest>,
    String,
    String,
    Option<BTreeSet<String>>,
) {
    const MAX_FILTERED_PROCESSES: usize = 16;
    if requested_scope != "impacted" {
        return full_execution_requests(targets, "full scope requested by caller");
    }
    if !selection.complete() {
        return full_execution_requests(
            targets,
            &format!(
                "impacted selection widened: uncovered obligations: {}",
                selection.uncovered_all.join(", ")
            ),
        );
    }
    if selection.selected.is_empty() {
        return full_execution_requests(
            targets,
            "impacted selection widened: no executable tests were selected",
        );
    }
    let mut grouped = FilterGroups::new();
    let mut executed = BTreeSet::new();
    for selected in &selection.selected {
        if browser_paths.contains(selected) {
            executed.insert(selected.clone());
            continue;
        }
        let absolute = repo.join(selected);
        if !absolute.is_file() {
            return full_execution_requests(
                targets,
                &format!("impacted selection widened: selected test `{selected}` is missing"),
            );
        }
        let mut matching = targets
            .iter()
            .filter(|target| absolute.starts_with(&target.cwd))
            .collect::<Vec<_>>();
        matching.sort_by_key(|target| std::cmp::Reverse(target.cwd.components().count()));
        let Some(target) = matching
            .into_iter()
            .find(|target| target_accepts_filter(target, selected))
        else {
            return full_execution_requests(
                targets,
                &format!(
                    "impacted selection widened: selected test `{selected}` has no filterable registered executor"
                ),
            );
        };
        let filter = absolute
            .strip_prefix(&target.cwd)
            .ok()
            .map(|path| normalize_path(&path.to_string_lossy()))
            .filter(|path| !path.is_empty());
        let Some(filter) = filter else {
            return full_execution_requests(
                targets,
                &format!(
                    "impacted selection widened: selected test `{selected}` cannot be expressed as a safe runner filter"
                ),
            );
        };
        let cwd = target.cwd.display().to_string();
        grouped
            .entry((target.executor.as_str().to_owned(), cwd))
            .or_insert_with(|| (target.clone(), Vec::new()))
            .1
            .push((filter, selected.clone()));
        executed.insert(selected.clone());
    }
    let requests = batch_filter_groups(grouped);
    if requests.len() > MAX_FILTERED_PROCESSES {
        return full_execution_requests(
            targets,
            &format!(
                "impacted selection widened: {} batched processes exceed the safe process-amplification limit {MAX_FILTERED_PROCESSES}",
                requests.len()
            ),
        );
    }
    if requests.is_empty() && executed.is_empty() {
        full_execution_requests(
            targets,
            "impacted selection widened: selection produced no runnable requests",
        )
    } else {
        let process_count = requests.len();
        let reason = format!(
            "complete selection mapped {} test paths to {process_count} bounded runner {}",
            executed.len(),
            if process_count == 1 {
                "process"
            } else {
                "processes"
            }
        );
        (requests, "impacted".into(), reason, Some(executed))
    }
}

fn batch_filter_groups(grouped: FilterGroups) -> Vec<ExecutionRequest> {
    const MAX_FILTERS_PER_PROCESS: usize = 128;
    const MAX_FILTER_BYTES_PER_PROCESS: usize = 24 * 1024;

    let mut requests = Vec::new();
    for (_, (target, pairs)) in grouped {
        let mut filters = Vec::new();
        let mut selected_tests = Vec::new();
        let mut filter_bytes = 0;
        for (filter, selected) in pairs {
            let next_bytes = filter_bytes + filter.len() + 1;
            if !filters.is_empty()
                && (filters.len() >= MAX_FILTERS_PER_PROCESS
                    || next_bytes > MAX_FILTER_BYTES_PER_PROCESS)
            {
                requests.push(ExecutionRequest {
                    target: target.clone(),
                    filters: std::mem::take(&mut filters),
                    selected_tests: std::mem::take(&mut selected_tests),
                });
                filter_bytes = 0;
            }
            filter_bytes += filter.len() + 1;
            filters.push(filter);
            selected_tests.push(selected);
        }
        if !filters.is_empty() {
            requests.push(ExecutionRequest {
                target,
                filters,
                selected_tests,
            });
        }
    }
    requests
}

fn supports_path_filters(executor: &str) -> bool {
    matches!(
        executor,
        "vitest" | "storybook-vitest" | "storybook-vitest-v8" | "jest" | "bun-test" | "playwright"
    )
}

fn target_accepts_filter(target: &ExecutorTarget, path: &str) -> bool {
    if is_story_path(path) {
        matches!(
            target.executor.as_str(),
            "storybook-vitest" | "storybook-vitest-v8"
        )
    } else {
        !matches!(
            target.executor.as_str(),
            "storybook-vitest" | "storybook-vitest-v8"
        ) && supports_path_filters(target.executor.as_str())
    }
}

fn available_test_paths(
    repo: &Path,
    targets: &[ExecutorTarget],
    browser_paths: &BTreeSet<String>,
) -> Result<BTreeSet<String>, BusError> {
    let raw = String::from_utf8(git_output(
        repo,
        &[
            "ls-files".into(),
            "--cached".into(),
            "--others".into(),
            "--exclude-standard".into(),
            "-z".into(),
        ],
    )?)
    .map_err(|err| BusError::Intelligence(format!("Git paths are not UTF-8: {err}")))?;
    let mut paths = raw
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(normalize_path)
        .filter(|path| is_test_path(path))
        .filter(|path| repo.join(path).is_file())
        .filter(|path| {
            let absolute = repo.join(path);
            targets.iter().any(|target| {
                target_accepts_filter(target, path) && absolute.starts_with(&target.cwd)
            })
        })
        .collect::<BTreeSet<_>>();
    paths.extend(browser_paths.iter().cloned());
    Ok(paths)
}

fn full_execution_requests(
    targets: &[ExecutorTarget],
    reason: &str,
) -> (
    Vec<ExecutionRequest>,
    String,
    String,
    Option<BTreeSet<String>>,
) {
    (
        targets
            .iter()
            .filter(|target| {
                !matches!(
                    target.executor.as_str(),
                    "storybook-vitest" | "storybook-vitest-v8"
                ) || !targets.iter().any(|candidate| {
                    candidate.cwd == target.cwd && candidate.executor.as_str() == "vitest"
                })
            })
            .cloned()
            .map(|target| ExecutionRequest {
                target,
                filters: Vec::new(),
                selected_tests: Vec::new(),
            })
            .collect(),
        "all".into(),
        reason.into(),
        None,
    )
}

fn execute_full_targets(
    executors: &ExecutorRegistry,
    repo: &Path,
    targets: &[ExecutorTarget],
    cancel: &Arc<AtomicBool>,
) -> Result<Vec<ExecutorRecord>, BusError> {
    let mut records = Vec::new();
    for target in targets {
        std::fs::create_dir_all(target.cwd.join(".weavatrix-quality")).map_err(|err| {
            BusError::Runtime(format!(
                "cannot prepare runner evidence directory in {}: {err}",
                target.cwd.display()
            ))
        })?;
        clear_generated_runner_artifacts(&target.cwd)?;
        let prepared = executors
            .prepare(PrepareRequest {
                executor: target.executor.clone(),
                cwd: target.cwd.clone(),
                filters: Vec::new(),
                extra: BTreeMap::new(),
                limits: default_limits(),
                cancel: Arc::clone(cancel),
            })
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        let started = SystemTime::now();
        let mut record = match executors.execute(&prepared) {
            Ok(ExecutionResult {
                status_code,
                stdout,
                stderr,
            }) => ExecutorRecord {
                executor: target.executor.as_str().to_owned(),
                cwd: relative_or_display(repo, &target.cwd),
                selection: Vec::new(),
                status_code,
                passed: status_code == Some(0),
                error: None,
                stdout,
                stderr,
                artifacts: Vec::new(),
            },
            Err(err) => ExecutorRecord {
                executor: target.executor.as_str().to_owned(),
                cwd: relative_or_display(repo, &target.cwd),
                selection: Vec::new(),
                status_code: None,
                passed: false,
                error: Some(err.to_string()),
                stdout: Vec::new(),
                stderr: Vec::new(),
                artifacts: Vec::new(),
            },
        };
        attach_normalized_artifacts(repo, &target.cwd, started, &mut record);
        clear_generated_runner_artifacts(&target.cwd)?;
        records.push(record);
    }
    Ok(records)
}

fn test_path_from_node_id(id: &str) -> Option<String> {
    let raw = id
        .strip_prefix("file:")
        .or_else(|| id.strip_prefix("symbol:"))
        .unwrap_or(id);
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
        || file.contains(".stories.")
}

fn is_story_path(path: &str) -> bool {
    normalize_path(path)
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase()
        .contains(".stories.")
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
    bindings: &[TestBinding],
    records: &[ExecutorRecord],
    run_id: &RunId,
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
    run_evidence_policy: &str,
) -> Result<StoredObligationExecutionMap, BusError> {
    let mut obligations = BTreeMap::<String, BTreeSet<StoredObligationExecution>>::new();
    for record in records {
        for artifact in record
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == "normalized-test-run")
        {
            let normalized: NormalizedTestRun =
                serde_json::from_slice(&artifact.bytes).map_err(|err| {
                    BusError::Runtime(format!(
                        "cannot decode normalized evidence from {}: {err}",
                        artifact.path
                    ))
                })?;
            for binding in bindings.iter().filter(|binding| {
                binding.case.is_some()
                    && binding
                        .runner
                        .as_deref()
                        .is_none_or(|runner| runner == record.executor)
            }) {
                for case in normalized.cases.iter().filter(|case| {
                    binding.case.as_deref() == Some(case.name.as_str())
                        && normalized_suite_matches(repo, record, binding, &case.suite)
                }) {
                    let evidence = StoredObligationExecution {
                        executor: record.executor.clone(),
                        path: binding.path.clone(),
                        suite: case.suite.clone(),
                        case: case.name.clone(),
                        status: normalized_status(case.status).into(),
                        invocation_passed: record.passed,
                        assertion: None,
                        observation: None,
                    };
                    for obligation in &binding.obligations {
                        obligations
                            .entry(obligation.clone())
                            .or_default()
                            .insert(evidence.clone());
                    }
                }
            }
        }
    }

    for (program_index, (configured, run)) in browser_runs.iter().enumerate() {
        for assertion in &run.assertions {
            let status = match assertion.status {
                BrowserAssertionStatus::Passed => "passed",
                BrowserAssertionStatus::Contradicted => "contradicted",
                BrowserAssertionStatus::Failed => "failed",
            };
            let observation = (run_evidence_policy != "none").then(|| {
                format!(
                    "artifact-{}-browser-{program_index}-observation-{}",
                    run_id.as_str(),
                    assertion.observation
                )
            });
            obligations
                .entry(assertion.obligation.clone())
                .or_default()
                .insert(StoredObligationExecution {
                    executor: "playwright-browser".into(),
                    path: configured.path.clone(),
                    suite: configured.path.clone(),
                    case: run.program.clone(),
                    status: status.into(),
                    invocation_passed: run.passed,
                    assertion: Some(format!("step:{}", assertion.step)),
                    observation,
                });
        }
    }

    Ok(StoredObligationExecutionMap {
        schema_v: 2,
        obligations: obligations
            .into_iter()
            .map(|(obligation, evidence)| (obligation, evidence.into_iter().collect()))
            .collect(),
    })
}

fn normalized_suite_matches(
    repo: &Path,
    record: &ExecutorRecord,
    binding: &TestBinding,
    observed_suite: &str,
) -> bool {
    let expected = binding.suite.as_deref().unwrap_or(&binding.path);
    if normalize_path(observed_suite) == normalize_path(expected) {
        return true;
    }
    let observed = Path::new(observed_suite);
    let observed = if observed.is_absolute() {
        observed.to_path_buf()
    } else {
        repo.join(&record.cwd).join(observed)
    };
    let expected = repo.join(&binding.path);
    std::fs::canonicalize(observed)
        .ok()
        .zip(std::fs::canonicalize(expected).ok())
        .is_some_and(|(observed, expected)| observed == expected)
}

fn normalized_status(status: TestStatus) -> &'static str {
    match status {
        TestStatus::Pass => "passed",
        TestStatus::Fail => "failed",
        TestStatus::Skip => "skipped",
        TestStatus::Error => "error",
    }
}

#[allow(
    clippy::if_not_else,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
fn persist_delta_triangle(
    store: &Store,
    run_id: &RunId,
    compiled: &Compiled,
    changed: &ChangedFiles,
    graph_diff: &Value,
    head_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
    base_replay: Result<BaseBrowserReplay, BusError>,
    run_evidence_policy: &str,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let spec_changed = changed.changes_openspec_change(&compiled.change);
    let code_changed = graph_diff_has_code_delta(graph_diff)?;
    let mut measured_programs = 0_u64;
    let mut changed_programs = Vec::new();
    let mut readings = Vec::new();
    let mut findings = Vec::new();
    let mut unmeasured_programs = Vec::new();
    let mut program_deltas = Vec::new();
    let mut base_revision = None;
    let mut replay_limitation = None;

    match base_replay {
        Err(error) => {
            replay_limitation = Some(error.to_string());
            unmeasured_programs.extend(
                head_runs
                    .iter()
                    .map(|(configured, _)| configured.program.id.to_string()),
            );
        }
        Ok(base) => {
            base_revision = Some(base.revision.to_string());
            if base.runs.len() != head_runs.len() {
                unmeasured_programs.extend(
                    head_runs
                        .iter()
                        .map(|(configured, _)| configured.program.id.to_string()),
                );
            } else {
                for (program_index, ((configured, head), base_run)) in
                    head_runs.iter().zip(&base.runs).enumerate()
                {
                    let program = configured.program.id.to_string();
                    let base_handles = persist_base_browser_observations(
                        store,
                        run_id,
                        program_index,
                        base_run,
                        run_evidence_policy != "none",
                        handles,
                    )?;
                    let head_handles = if run_evidence_policy == "none" {
                        Vec::new()
                    } else {
                        (0..head.observations.len())
                            .map(|index| {
                                format!(
                                    "artifact-{}-browser-{program_index}-observation-{index}",
                                    run_id.as_str()
                                )
                            })
                            .collect::<Vec<_>>()
                    };
                    if base_run.program != program
                        || head.program != program
                        || !browser_measurement_complete(base_run, &configured.program)
                        || !browser_measurement_complete(head, &configured.program)
                        || base_run.observations.len() != head.observations.len()
                    {
                        unmeasured_programs.push(program.clone());
                        program_deltas.push(json!({
                            "program": program,
                            "measured": false,
                            "base_passed": base_run.passed,
                            "head_passed": head.passed,
                            "base_observations": base_handles,
                            "head_observations": head_handles,
                        }));
                        continue;
                    }
                    let delta =
                        paired_observation_delta(&base_run.observations, &head.observations);
                    let triangle = join_triangle(
                        SpecDelta {
                            changed: spec_changed,
                        },
                        CodeDelta {
                            changed: code_changed,
                        },
                        &delta,
                        &program,
                    );
                    measured_programs = measured_programs.saturating_add(1);
                    if delta.changed() {
                        changed_programs.push(program.clone());
                    }
                    readings.push(triangle.reading.as_str().to_owned());
                    for finding in &triangle.findings {
                        findings.push(json!({
                            "check": finding.check.as_str(),
                            "severity": severity_token(finding.severity),
                            "program": program,
                            "detail": finding.summary,
                        }));
                    }
                    program_deltas.push(json!({
                        "program": program,
                        "measured": true,
                        "reading": triangle.reading.as_str(),
                        "behavior_changed": delta.changed(),
                        "first_behavior_axis": triangle.first_behavior_axis,
                        "pixel_compared": triangle.pixel_compared,
                        "changed_axes": delta.axes.iter().map(|axis| axis.axis.as_str()).collect::<Vec<_>>(),
                        "base_observations": base_handles,
                        "head_observations": head_handles,
                    }));
                }
            }
        }
    }
    changed_programs.sort();
    changed_programs.dedup();
    unmeasured_programs.sort();
    unmeasured_programs.dedup();
    readings.sort();
    let has_blocking = findings
        .iter()
        .any(|finding| finding.get("severity").and_then(Value::as_str) == Some("error"));
    let state = if has_blocking {
        "blocking"
    } else if !unmeasured_programs.is_empty() {
        "unmeasured"
    } else if findings.is_empty() {
        "clean"
    } else {
        "warnings"
    };
    let document = json!({
        "schema_v": 1,
        "state": state,
        "spec_changed": spec_changed,
        "code_changed": code_changed,
        "behavior_changed": !changed_programs.is_empty(),
        "measured_programs": measured_programs,
        "changed_programs": changed_programs,
        "readings": readings,
        "findings": findings,
        "unmeasured_programs": unmeasured_programs,
        "base_revision": base_revision,
        "replay_limitation": replay_limitation,
        "programs": program_deltas,
        "runtime_llm_tokens": 0,
    });
    put_json_run_artifact(
        store,
        run_id,
        &format!("artifact-{}-delta-triangle", run_id.as_str()),
        DELTA_TRIANGLE_KIND,
        &document,
        handles,
    )
}

fn browser_measurement_complete(result: &BrowserProgramRun, program: &TestProgram) -> bool {
    let all_steps_observed = result.action_spans.len() == program.steps.len()
        && result.observations.len() == program.steps.len().saturating_add(1);
    all_steps_observed
        && (result.passed
            || result
                .failure
                .as_deref()
                .is_some_and(|failure| failure.starts_with("assertion_failed:")))
}

fn persist_base_browser_observations(
    store: &Store,
    run_id: &RunId,
    program_index: usize,
    result: &BrowserProgramRun,
    keep: bool,
    handles: &mut Vec<String>,
) -> Result<Vec<String>, BusError> {
    let mut observation_handles = Vec::new();
    if !keep {
        return Ok(observation_handles);
    }
    for (index, observation) in result.observations.iter().enumerate() {
        let id = format!(
            "artifact-{}-delta-base-{program_index}-observation-{index}",
            run_id.as_str()
        );
        put_json_run_artifact(
            store,
            run_id,
            &id,
            "base-browser-observation",
            observation,
            handles,
        )?;
        observation_handles.push(id);
    }
    let evidence = json!({
        "schema_v": 1,
        "program": result.program,
        "passed": result.passed,
        "observations": observation_handles,
        "action_spans": result.action_spans,
    });
    put_json_run_artifact(
        store,
        run_id,
        &format!(
            "artifact-{}-delta-base-{program_index}-program",
            run_id.as_str()
        ),
        "base-browser-program-evidence",
        &evidence,
        handles,
    )?;
    Ok(observation_handles)
}

fn paired_observation_delta(
    base: &[wvq_runtime::Observation],
    head: &[wvq_runtime::Observation],
) -> BehaviorDelta {
    let mut axes = BTreeMap::<DiffAxis, (Vec<String>, Vec<String>)>::new();
    let mut first_structured = None;
    for (index, (base, head)) in base.iter().zip(head).enumerate() {
        let delta = behavior_delta(
            &StructuredView::from_replay(base, None),
            &StructuredView::from_replay(head, None),
        );
        first_structured = first_structured.or(delta.first_structured);
        for axis in delta.axes {
            let entry = axes.entry(axis.axis).or_default();
            entry
                .0
                .push(format!("{index}:{}", sha256_hex(axis.base.as_bytes())));
            entry
                .1
                .push(format!("{index}:{}", sha256_hex(axis.head.as_bytes())));
        }
    }
    if first_structured.is_some() {
        axes.remove(&DiffAxis::Pixel);
    }
    BehaviorDelta {
        axes: axes
            .into_iter()
            .map(|(axis, (base, head))| AxisDelta {
                axis,
                base: base.join(","),
                head: head.join(","),
            })
            .collect(),
        first_structured,
        pixel_compared: first_structured.is_none(),
    }
}

fn graph_diff_has_code_delta(diff: &Value) -> Result<bool, BusError> {
    ensure_complete_diff(diff)?;
    Ok([
        "nodes_added",
        "nodes_removed",
        "nodes_changed",
        "edges_added",
        "edges_removed",
    ]
    .into_iter()
    .any(|count| {
        diff.pointer(&format!("/counts/{count}"))
            .and_then(Value::as_u64)
            .is_some_and(|value| value > 0)
    }))
}

fn severity_token(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "info",
        Severity::Warn => "warn",
        Severity::Error => "error",
    }
}

fn persist_browser_runs(
    store: &Store,
    run_id: &RunId,
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
    run_evidence_policy: &str,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let keep_normalized = run_evidence_policy != "none";
    for (program_index, (configured, result)) in browser_runs.iter().enumerate() {
        persist_browser_run(
            store,
            run_id,
            program_index,
            configured,
            result,
            run_evidence_policy,
            keep_normalized,
            handles,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn persist_browser_run(
    store: &Store,
    run_id: &RunId,
    program_index: usize,
    configured: &ConfiguredBrowserProgram,
    result: &BrowserProgramRun,
    run_evidence_policy: &str,
    keep_normalized: bool,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let token = safe_file_token(configured.program.id.as_str());
    let observation_handles = persist_browser_observations(
        store,
        run_id,
        program_index,
        result,
        keep_normalized,
        handles,
    )?;
    persist_browser_files(
        store,
        run_id,
        program_index,
        result,
        keep_normalized,
        handles,
    )?;
    let assertions = stored_browser_assertions(result, keep_normalized, &observation_handles)?;
    let evidence = StoredBrowserProgramEvidence {
        schema_v: 2,
        program: configured.program.id.to_string(),
        asserted: result.asserted.clone(),
        contradicted: result.contradicted.clone(),
        assertions,
        present: browser_evidence_kinds(configured, result, run_evidence_policy),
        observations: observation_handles,
    };
    put_json_run_artifact(
        store,
        run_id,
        &format!(
            "artifact-{}-browser-{program_index}-{token}",
            run_id.as_str()
        ),
        "browser-program-evidence",
        &evidence,
        handles,
    )
}

fn persist_browser_observations(
    store: &Store,
    run_id: &RunId,
    program_index: usize,
    result: &BrowserProgramRun,
    keep: bool,
    handles: &mut Vec<String>,
) -> Result<Vec<String>, BusError> {
    let mut observation_handles = Vec::new();
    if !keep {
        return Ok(observation_handles);
    }
    for (index, observation) in result.observations.iter().enumerate() {
        let id = format!(
            "artifact-{}-browser-{program_index}-observation-{index}",
            run_id.as_str()
        );
        put_json_run_artifact(
            store,
            run_id,
            &id,
            "browser-observation",
            observation,
            handles,
        )?;
        observation_handles.push(id);
    }
    Ok(observation_handles)
}

fn persist_browser_files(
    store: &Store,
    run_id: &RunId,
    program_index: usize,
    result: &BrowserProgramRun,
    keep: bool,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    for (index, path) in result.screenshot_paths.iter().enumerate() {
        if keep {
            let bytes = std::fs::read(path).map_err(|err| {
                BusError::Runtime(format!(
                    "cannot import browser screenshot {}: {err}",
                    path.display()
                ))
            })?;
            put_run_artifact(
                store,
                run_id,
                &format!(
                    "artifact-{}-browser-{program_index}-screenshot-{index}",
                    run_id.as_str()
                ),
                "screenshot",
                &bytes,
                handles,
            )?;
        }
        remove_browser_evidence_file(path)?;
    }
    if let Some(path) = &result.trace_path {
        if keep {
            let bytes = std::fs::read(path).map_err(|err| {
                BusError::Runtime(format!(
                    "cannot import browser trace {}: {err}",
                    path.display()
                ))
            })?;
            put_run_artifact(
                store,
                run_id,
                &format!("artifact-{}-browser-{program_index}-trace", run_id.as_str()),
                "playwright-trace",
                &bytes,
                handles,
            )?;
        }
        remove_browser_evidence_file(path)?;
    }
    Ok(())
}

fn stored_browser_assertions(
    result: &BrowserProgramRun,
    keep: bool,
    observation_handles: &[String],
) -> Result<Vec<StoredBrowserAssertionEvidence>, BusError> {
    result
        .assertions
        .iter()
        .map(|assertion| {
            let status = match assertion.status {
                BrowserAssertionStatus::Passed => "passed",
                BrowserAssertionStatus::Contradicted => "contradicted",
                BrowserAssertionStatus::Failed => "failed",
            };
            let observation = if keep {
                Some(
                    observation_handles
                        .get(assertion.observation)
                        .cloned()
                        .ok_or_else(|| {
                            BusError::Runtime(format!(
                                "browser assertion step {} references missing observation {}",
                                assertion.step, assertion.observation
                            ))
                        })?,
                )
            } else {
                None
            };
            Ok(StoredBrowserAssertionEvidence {
                obligation: assertion.obligation.clone(),
                step: assertion.step,
                status: status.into(),
                observation,
            })
        })
        .collect()
}

const BEHAVIOR_SAMPLE_LIMIT: usize = 500;
const BEHAVIOR_PROGRAM_SAMPLE_LIMIT: usize = 100;

/// Largest UI evidence document written for one run.
const MAX_UI_ARTIFACT_BYTES: usize = 4 * 1024 * 1024;

/// How many findings of one class reach the bounded verdict projection.
///
/// The full list stays in the CAS artifact; the reply carries enough to act on
/// without paying for hundreds of near-identical entries.
const MAX_UI_REPLY_FINDINGS: usize = 25;

/// Serialize the ratchet into the exact document `quality_verify` reads.
///
/// The axis state is decided here, once, from the classified delta: a new or
/// returned error blocks, a truncated or one-sided measurement is unmeasured,
/// warnings are warnings, and everything else is clean. Old debt is counted and
/// never listed.
fn ui_delta_document(delta: &UiIntegrityDelta) -> Value {
    let state = if delta.blocks() {
        "blocking"
    } else if delta.truncated || delta.responsive_truncated || !delta.unmeasured_states.is_empty() {
        "unmeasured"
    } else if delta.new.is_empty() && delta.returned.is_empty() && delta.existing.is_empty() {
        "clean"
    } else if delta.new.is_empty() && delta.returned.is_empty() {
        // Only pre-existing debt survived: recorded, not held against the change.
        "clean"
    } else {
        "warnings"
    };
    json!({
        "schema_v": 1,
        "state": state,
        "new": ui_finding_refs_with_intervals(&delta.new, &delta.responsive_intervals, UiFindingState::New),
        "returned": ui_finding_refs_with_intervals(&delta.returned, &delta.responsive_intervals, UiFindingState::Returned),
        "existing": delta.existing.len(),
        "fixed": delta.fixed.len(),
        "excepted": delta.excepted.len(),
        "unmeasured_states": delta.unmeasured_states,
        "truncated": delta.truncated,
        "expired_policy": delta.expired_policy,
        "responsive_intervals": delta.responsive_intervals,
        "responsive_truncated": delta.responsive_truncated,
        "runtime_llm_tokens": 0,
    })
}

fn responsive_probe_incomplete(probe: &ResponsiveProbe) -> bool {
    probe.delta.truncated || !probe.delta.unmeasured_states.is_empty()
}

fn ui_finding_refs_with_intervals(
    findings: &[UiIntegrityFinding],
    intervals: &[wvq_ui::ResponsiveFailureInterval],
    state: UiFindingState,
) -> Vec<Value> {
    let mut out = ui_finding_refs(findings);
    out.extend(
        intervals
            .iter()
            .filter(|interval| interval.state == state)
            .take(MAX_UI_REPLY_FINDINGS.saturating_sub(out.len()))
            .map(|interval| {
                let height = interval
                    .finding
                    .viewport
                    .split_once('x')
                    .map_or("?", |(_, height)| height);
                json!({
                    "check": interval.finding.check.id(),
                    "severity": match interval.finding.severity {
                        Severity::Info => "info",
                        Severity::Warn => "warn",
                        Severity::Error => "error",
                    },
                    "subject": interval.finding.subject,
                    "route": interval.finding.route,
                    "viewport": format!("{}-{}x{height}", interval.first_width, interval.last_width),
                    "detail": format!(
                        "{}; responsive failure interval {}..={} px (lower exact: {}, upper exact: {})",
                        interval.finding.detail,
                        interval.first_width,
                        interval.last_width,
                        interval.lower_boundary_exact,
                        interval.upper_boundary_exact,
                    ),
                })
            }),
    );
    out
}

fn ui_finding_refs(findings: &[UiIntegrityFinding]) -> Vec<Value> {
    findings
        .iter()
        .take(MAX_UI_REPLY_FINDINGS)
        .map(|finding| {
            json!({
                "check": finding.check.id(),
                "severity": match finding.severity {
                    Severity::Info => "info",
                    Severity::Warn => "warn",
                    Severity::Error => "error",
                },
                "subject": finding.subject,
                "route": finding.route,
                "viewport": finding.viewport,
                "detail": finding.detail,
            })
        })
        .collect()
}

/// Turn one run's collected layout snapshots into stored UI-integrity evidence.
///
/// Three artifacts come out of this: the raw bounded snapshots, a compact
/// hit-test map for `quality_explain`, and the findings the detectors produced.
/// All three are CAS handles; none of them is ever inlined into an MCP reply.
///
/// Returns the head-side snapshot so a base/head comparison can use it.
fn persist_ui_integrity(
    store: &Store,
    run_id: &RunId,
    revision: &RevisionId,
    policy: &UiIntegrityPolicy,
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
    handles: &mut Vec<String>,
) -> Result<Option<UiIntegritySnapshot>, BusError> {
    if !policy.enabled {
        return Ok(None);
    }
    let collected = analyse_ui_snapshots(revision, policy, browser_runs)?;
    if collected.snapshot.measured_states.is_empty() && !collected.snapshot.truncated {
        // No browser program produced a snapshot: this run has no UI surface.
        return Ok(None);
    }
    put_bounded_ui_artifact(
        store,
        run_id,
        &format!("artifact-{}-ui-layout-snapshot", run_id.as_str()),
        "ui-layout-snapshot",
        &collected.layouts,
        handles,
    )?;
    put_bounded_ui_artifact(
        store,
        run_id,
        &format!("artifact-{}-ui-hit-test-map", run_id.as_str()),
        "ui-hit-test-map",
        &collected.hit_test_map,
        handles,
    )?;
    put_bounded_ui_artifact(
        store,
        run_id,
        &format!("artifact-{}-ui-integrity-findings", run_id.as_str()),
        "ui-integrity-findings",
        &json!({
            "schema_v": 1,
            "revision": revision.as_str(),
            "measured_states": collected.snapshot.measured_states,
            "findings": collected.snapshot.findings,
            "responsive_breakpoints": collected.snapshot.responsive_breakpoints,
            "responsive_breakpoints_incomplete": collected.snapshot.responsive_breakpoints_incomplete,
            "truncated": collected.snapshot.truncated,
            "runtime_llm_tokens": 0,
        }),
        handles,
    )?;
    Ok(Some(collected.snapshot))
}

struct CollectedUi {
    snapshot: UiIntegritySnapshot,
    layouts: Value,
    hit_test_map: Value,
}

/// Decode, validate, and analyse every layout snapshot one run collected.
///
/// A snapshot the collector could not take, or one it flagged as unsettled or
/// truncated, marks the whole measurement incomplete. That is deliberately
/// louder than dropping it: an unmeasured state must not read as a clean one.
fn analyse_ui_snapshots(
    revision: &RevisionId,
    policy: &UiIntegrityPolicy,
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
) -> Result<CollectedUi, BusError> {
    let mut snapshot = UiIntegritySnapshot {
        revision: revision.to_string(),
        ..UiIntegritySnapshot::default()
    };
    let mut layouts = Vec::new();
    let mut hit_map = Vec::new();
    for (configured, result) in browser_runs {
        let duplicate_mutations = wvq_runtime::duplicate_mutation_requests(result);
        if result
            .observations
            .iter()
            .any(|observation| observation.network_requests_truncated)
            || !matches!(
                configured.program.evidence_policy.network,
                CaptureWhen::Always
            )
        {
            snapshot.truncated = true;
        }
        for evidence in &result.ui_snapshots {
            if !evidence.limitations.is_empty() || evidence.snapshot.is_null() {
                snapshot.truncated = true;
            }
            if evidence.snapshot.is_null() {
                continue;
            }
            let layout: LayoutSnapshot = serde_json::from_value(evidence.snapshot.clone())
                .map_err(|err| {
                    BusError::Runtime(format!(
                        "browser returned a malformed layout snapshot for {} step {}: {err}",
                        result.program, evidence.step
                    ))
                })?;
            if layout.revision.as_str() != revision.as_str() {
                return Err(BusError::Ambiguous(format!(
                    "layout snapshot for {} claims revision `{}`, the run is at `{revision}`",
                    result.program, layout.revision
                )));
            }
            let output = detect_ui(&layout, policy)
                .map_err(|err| BusError::Runtime(format!("ui integrity: {err}")))?;
            snapshot.truncated |= output.truncated;
            snapshot
                .responsive_breakpoints
                .extend(layout.responsive_breakpoints.iter().copied());
            snapshot.responsive_breakpoints_incomplete |= !layout.responsive_breakpoints_complete;
            snapshot.measured_states.insert(layout.state_key());
            snapshot.findings.extend(output.findings);
            snapshot.findings.extend(
                duplicate_mutations
                    .iter()
                    .filter(|duplicate| duplicate.step == evidence.step)
                    .map(|duplicate| duplicate_mutation_finding(&layout, duplicate)),
            );
            hit_map.push(hit_test_summary(&layout));
            layouts.push(serde_json::to_value(&layout).map_err(|err| {
                BusError::Runtime(format!("cannot encode layout snapshot: {err}"))
            })?);
        }
    }
    wvq_ui::sort_findings(&mut snapshot.findings);
    Ok(CollectedUi {
        layouts: json!({
            "schema_v": 1,
            "revision": revision.as_str(),
            "snapshots": layouts,
        }),
        hit_test_map: json!({
            "schema_v": 1,
            "revision": revision.as_str(),
            "targets": hit_map,
        }),
        snapshot,
    })
}

fn duplicate_mutation_finding(
    layout: &LayoutSnapshot,
    duplicate: &wvq_runtime::DuplicateMutationRequest,
) -> UiIntegrityFinding {
    let count = u32::try_from(duplicate.sequences.len()).unwrap_or(u32::MAX);
    UiIntegrityFinding {
        check: wvq_ui::UiCheck::DuplicateMutationRequest,
        severity: Severity::Error,
        state: layout.state_key(),
        route: layout.route.clone(),
        viewport: format!("{}x{}", layout.viewport.width, layout.viewport.height),
        subject: format!("{} {}", duplicate.method, duplicate.url),
        counterpart: None,
        component_hint: None,
        nodes: Vec::new(),
        evidence: wvq_ui::UiEvidence {
            duplicate_count: count,
            ..wvq_ui::UiEvidence::default()
        },
        detail: format!(
            "one action at step {} emitted the same mutating request {count} times (request sequences {})",
            duplicate.step,
            duplicate
                .sequences
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Per-target hit-test totals: what was probed, what got through, and what
/// intercepted the rest. Small enough to read, exact enough to act on.
fn hit_test_summary(layout: &LayoutSnapshot) -> Value {
    let index = layout.index();
    let mut targets: BTreeMap<String, (u32, u32, BTreeMap<String, u32>)> = BTreeMap::new();
    for sample in &layout.hit_tests {
        let entry = targets
            .entry(sample.target.to_string())
            .or_insert_with(|| (0, 0, BTreeMap::new()));
        entry.0 += 1;
        match &sample.topmost {
            Some(topmost) if index.is_self_or_descendant(topmost, &sample.target) => entry.1 += 1,
            Some(topmost) => {
                *entry
                    .2
                    .entry(
                        index
                            .node(topmost)
                            .map_or_else(|| topmost.to_string(), wvq_ui::UiNode::semantic_identity),
                    )
                    .or_default() += 1;
            }
            None => entry.1 += 1,
        }
    }
    json!({
        "state": layout.state_key(),
        "route": layout.route,
        "viewport": layout.viewport.label(),
        "targets": targets
            .into_iter()
            .map(|(target, (samples, received, blockers))| {
                json!({
                    "target": index
                        .node(&wvq_ui::UiNodeId::new(&target).unwrap_or_default())
                        .map_or(target.clone(), wvq_ui::UiNode::semantic_identity),
                    "node": target,
                    "samples": samples,
                    "received_events": received,
                    "blockers": blockers,
                })
            })
            .collect::<Vec<_>>(),
    })
}

fn put_bounded_ui_artifact(
    store: &Store,
    run_id: &RunId,
    raw_id: &str,
    kind: &str,
    value: &Value,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|err| BusError::Runtime(format!("cannot encode {kind}: {err}")))?;
    if bytes.len() > MAX_UI_ARTIFACT_BYTES {
        // Refuse rather than store a partial document: half a snapshot would be
        // analysed as though it were the whole page.
        return Err(BusError::Runtime(format!(
            "{kind} is {} bytes, over the {MAX_UI_ARTIFACT_BYTES}-byte ceiling; \
             lower ui_integrity.max_nodes",
            bytes.len()
        )));
    }
    put_run_artifact(store, run_id, raw_id, kind, &bytes, handles)
}

fn persist_browser_behavior(
    store: &Store,
    run_id: &RunId,
    revision: &RevisionId,
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
    handles: &mut Vec<String>,
) -> Result<BehaviorContributionSummary, BusError> {
    if browser_runs.is_empty() {
        return Ok(BehaviorContributionSummary::default());
    }

    let mut all_states = BTreeSet::new();
    let mut all_new_states = BTreeSet::new();
    let mut all_edges = BTreeSet::new();
    let mut all_new_edges = BTreeSet::new();
    let mut all_api_operations = BTreeSet::new();
    let mut programs = Vec::new();

    for (configured, result) in browser_runs {
        let contribution = persist_program_behavior(store, configured, result)?;
        all_states.extend(contribution.states.iter().cloned());
        all_new_states.extend(contribution.new_states.iter().cloned());
        all_edges.extend(contribution.edges.iter().cloned());
        all_new_edges.extend(contribution.new_edges.iter().cloned());
        all_api_operations.extend(contribution.api_operations.iter().cloned());
        programs.push(contribution.artifact);
    }

    let summary = BehaviorContributionSummary {
        states: u64::try_from(all_states.len()).unwrap_or(u64::MAX),
        new_states: u64::try_from(all_new_states.len()).unwrap_or(u64::MAX),
        edges: u64::try_from(all_edges.len()).unwrap_or(u64::MAX),
        new_edges: u64::try_from(all_new_edges.len()).unwrap_or(u64::MAX),
    };
    let (state_digests, state_digests_truncated) = bounded_set(&all_states, BEHAVIOR_SAMPLE_LIMIT);
    let (new_state_digests, new_state_digests_truncated) =
        bounded_set(&all_new_states, BEHAVIOR_SAMPLE_LIMIT);
    let (api_operations, api_operations_truncated) =
        bounded_set(&all_api_operations, BEHAVIOR_SAMPLE_LIMIT);
    let artifact = json!({
        "schema_v": 1,
        "run_id": run_id,
        "revision": revision,
        "state_count": summary.states,
        "new_state_count": summary.new_states,
        "edge_count": summary.edges,
        "new_edge_count": summary.new_edges,
        "state_digests": state_digests,
        "new_state_digests": new_state_digests,
        "api_operations": api_operations,
        "programs": programs,
        "coverage_status": "unmeasured",
        "coverage_nodes": [],
        "truncated": state_digests_truncated
            || new_state_digests_truncated
            || api_operations_truncated,
        "runtime_llm_tokens": 0,
    });
    put_json_run_artifact(
        store,
        run_id,
        &format!("artifact-{}-behavior-contribution", run_id.as_str()),
        "behavior-contribution",
        &artifact,
        handles,
    )?;
    Ok(summary)
}

fn persist_program_behavior(
    store: &Store,
    configured: &ConfiguredBrowserProgram,
    result: &BrowserProgramRun,
) -> Result<ProgramBehaviorContribution, BusError> {
    let mut states = BTreeSet::new();
    let mut new_states = BTreeSet::new();
    let mut edges = BTreeSet::new();
    let mut new_edges = BTreeSet::new();
    let mut api_operations = BTreeSet::new();
    let mut observation_states = BTreeMap::new();

    for (index, observation) in result.observations.iter().enumerate() {
        api_operations.extend(
            observation
                .network
                .iter()
                .map(|operation| bounded_network_operation(operation))
                .filter(|operation| !operation.is_empty()),
        );
        let Some((digest, body)) = normalized_behavior_state(observation)? else {
            continue;
        };
        let digest_text = digest.to_string();
        states.insert(digest_text.clone());
        if store
            .put_behavior_state(&digest, &body)
            .map_err(|err| BusError::Store(err.to_string()))?
        {
            new_states.insert(digest_text);
        }
        observation_states.insert(index, digest);
    }
    for span in &result.action_spans {
        if let (Some(previous_digest), Some(digest)) = (
            observation_states.get(&span.start_observation),
            observation_states.get(&span.end_observation),
        ) {
            let (key, inserted) =
                persist_behavior_edge(store, previous_digest, digest, &span.action)?;
            edges.insert(key.clone());
            if inserted {
                new_edges.insert(key);
            }
        }
        if let TestAction::ApiCall { operation, .. } = &span.action {
            api_operations.insert(operation.clone());
        }
    }

    let artifact = program_behavior_artifact(
        configured,
        result,
        &states,
        &new_states,
        &edges,
        &new_edges,
        &api_operations,
    );
    Ok(ProgramBehaviorContribution {
        states,
        new_states,
        edges,
        new_edges,
        api_operations,
        artifact,
    })
}

fn normalized_behavior_state(
    observation: &wvq_runtime::Observation,
) -> Result<Option<(ContentHash, Vec<u8>)>, BusError> {
    let Some(route) = observation
        .route
        .as_deref()
        .map(str::trim)
        .filter(|route| !route.is_empty())
    else {
        return Ok(None);
    };
    let state = BehaviorState {
        route: route.to_owned(),
        a11y_digest: observation.a11y_digest.clone(),
        viewport: observation.viewport.clone(),
        ..BehaviorState::default()
    };
    let body = state
        .canonical_json()
        .map_err(|err| BusError::Runtime(err.to_string()))?;
    let digest = state
        .digest()
        .map_err(|err| BusError::Runtime(err.to_string()))?;
    Ok(Some((digest, body)))
}

fn persist_behavior_edge(
    store: &Store,
    previous: &ContentHash,
    current: &ContentHash,
    action: &TestAction,
) -> Result<(String, bool), BusError> {
    let action = serde_json::to_string(action).map_err(|err| BusError::Runtime(err.to_string()))?;
    let key = format!("{previous}\0{action}\0{current}");
    let inserted = store
        .put_behavior_edge(previous, current, &action)
        .map_err(|err| BusError::Store(err.to_string()))?;
    Ok((key, inserted))
}

fn program_behavior_artifact(
    configured: &ConfiguredBrowserProgram,
    result: &BrowserProgramRun,
    states: &BTreeSet<String>,
    new_states: &BTreeSet<String>,
    edges: &BTreeSet<String>,
    new_edges: &BTreeSet<String>,
    api_operations: &BTreeSet<String>,
) -> Value {
    let (state_digests, state_digests_truncated) =
        bounded_set(states, BEHAVIOR_PROGRAM_SAMPLE_LIMIT);
    let (new_state_digests, new_state_digests_truncated) =
        bounded_set(new_states, BEHAVIOR_PROGRAM_SAMPLE_LIMIT);
    let (api_operations, api_operations_truncated) =
        bounded_set(api_operations, BEHAVIOR_PROGRAM_SAMPLE_LIMIT);
    let duplicate_mutations = wvq_runtime::duplicate_mutation_requests(result);
    let action_spans = result
        .action_spans
        .iter()
        .take(BEHAVIOR_PROGRAM_SAMPLE_LIMIT)
        .collect::<Vec<_>>();
    let duplicate_samples = duplicate_mutations
        .iter()
        .take(BEHAVIOR_PROGRAM_SAMPLE_LIMIT)
        .collect::<Vec<_>>();
    json!({
        "program": configured.program.id,
        "passed": result.passed,
        "obligations": configured.program.obligations,
        "state_count": states.len(),
        "new_state_count": new_states.len(),
        "edge_count": edges.len(),
        "new_edge_count": new_edges.len(),
        "state_digests": state_digests,
        "new_state_digests": new_state_digests,
        "api_operations": api_operations,
        "action_span_count": result.action_spans.len(),
        "action_spans": action_spans,
        "duplicate_mutation_request_count": duplicate_mutations.len(),
        "duplicate_mutation_requests": duplicate_samples,
        "network_request_evidence_truncated": result.observations.iter().any(|observation| observation.network_requests_truncated),
        "coverage_status": "unmeasured",
        "coverage_nodes": [],
        "truncated": state_digests_truncated
            || new_state_digests_truncated
            || api_operations_truncated
            || result.action_spans.len() > BEHAVIOR_PROGRAM_SAMPLE_LIMIT
            || duplicate_mutations.len() > BEHAVIOR_PROGRAM_SAMPLE_LIMIT
            || result.observations.iter().any(|observation| observation.network_requests_truncated),
    })
}

fn bounded_set(values: &BTreeSet<String>, limit: usize) -> (Vec<String>, bool) {
    (
        values.iter().take(limit).cloned().collect(),
        values.len() > limit,
    )
}

fn bounded_network_operation(operation: &str) -> String {
    let mut parts = operation.split_whitespace();
    let Some(method) = parts.next() else {
        return String::new();
    };
    let Some(raw_url) = parts.next() else {
        return operation.chars().take(512).collect();
    };
    let url = raw_url
        .split(['?', '#'])
        .next()
        .unwrap_or(raw_url)
        .chars()
        .take(400)
        .collect::<String>();
    let status = parts.next().unwrap_or_default();
    format!(
        "{} {} {}",
        method.chars().take(16).collect::<String>(),
        url,
        status
    )
    .trim_end()
    .to_owned()
}

fn browser_evidence_kinds(
    configured: &ConfiguredBrowserProgram,
    result: &BrowserProgramRun,
    run_evidence_policy: &str,
) -> Vec<EvidenceKind> {
    let failed = !result.passed;
    let policy = &configured.program.evidence_policy;
    let mut present = Vec::new();
    if result
        .observations
        .iter()
        .any(|observation| observation.a11y_digest.is_some())
    {
        present.push(EvidenceKind::Dom);
    }
    if browser_capture_active(policy.network, failed, run_evidence_policy) {
        present.push(EvidenceKind::Network);
    }
    if browser_capture_active(policy.console, failed, run_evidence_policy) {
        present.push(EvidenceKind::Console);
    }
    if browser_capture_active(policy.storage, failed, run_evidence_policy)
        && result
            .observations
            .iter()
            .any(|observation| observation.storage_available)
    {
        present.push(EvidenceKind::Storage);
    }
    if run_evidence_policy != "none" && !result.screenshot_paths.is_empty() {
        present.push(EvidenceKind::Screenshot);
    }
    if run_evidence_policy != "none" && result.trace_path.is_some() {
        present.push(EvidenceKind::Trace);
    }
    present.sort_by_key(|kind| format!("{kind:?}"));
    present.dedup();
    present
}

fn remove_browser_evidence_file(path: &Path) -> Result<(), BusError> {
    std::fs::remove_file(path).map_err(|err| {
        BusError::Runtime(format!(
            "cannot remove imported browser evidence {}: {err}",
            path.display()
        ))
    })
}

fn capture_active(policy: CaptureWhen, failed: bool) -> bool {
    matches!(policy, CaptureWhen::Always) || (failed && matches!(policy, CaptureWhen::OnFailure))
}

fn browser_capture_active(policy: CaptureWhen, failed: bool, run_policy: &str) -> bool {
    match run_policy {
        "standard" => capture_active(policy, failed),
        "minimal" => failed && !matches!(policy, CaptureWhen::Never),
        _ => false,
    }
}

fn cap_browser_evidence(program: &mut TestProgram, run_policy: &str) {
    let cap = |capture: CaptureWhen| match run_policy {
        "minimal" if matches!(capture, CaptureWhen::Always) => CaptureWhen::OnFailure,
        "standard" | "minimal" => capture,
        _ => CaptureWhen::Never,
    };
    program.evidence_policy.screenshot = cap(program.evidence_policy.screenshot);
    program.evidence_policy.trace = cap(program.evidence_policy.trace);
    program.evidence_policy.network = cap(program.evidence_policy.network);
    program.evidence_policy.console = cap(program.evidence_policy.console);
    program.evidence_policy.storage = cap(program.evidence_policy.storage);
}

fn safe_file_token(value: &str) -> String {
    let token = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .take(100)
        .collect::<String>();
    if token.is_empty() {
        "program".into()
    } else {
        token
    }
}

fn parse_obligation_execution_map(
    bytes: &[u8],
) -> Result<BTreeMap<String, Vec<StoredObligationExecution>>, BusError> {
    let stored: StoredObligationExecutionMap = serde_json::from_slice(bytes)
        .map_err(|err| BusError::Store(format!("invalid obligation execution map: {err}")))?;
    if stored.schema_v != 2 {
        return Err(BusError::Store(
            "unknown obligation execution map schema".into(),
        ));
    }
    for (obligation, entries) in &stored.obligations {
        if obligation.is_empty() {
            return Err(BusError::Store(
                "obligation execution map has an empty obligation identity".into(),
            ));
        }
        for entry in entries {
            if entry.executor.is_empty()
                || entry.path.is_empty()
                || entry.suite.is_empty()
                || entry.case.is_empty()
                || !matches!(
                    entry.status.as_str(),
                    "passed" | "failed" | "skipped" | "error" | "contradicted"
                )
                || entry.assertion.as_deref().is_some_and(str::is_empty)
                || entry.observation.as_deref().is_some_and(str::is_empty)
            {
                return Err(BusError::Store(format!(
                    "obligation execution map {obligation} has invalid exact evidence"
                )));
            }
        }
    }
    Ok(stored.obligations)
}

fn parse_revision_range_evidence(bytes: &[u8]) -> Result<RevisionRange, BusError> {
    let stored: StoredRevisionRangeEvidence = serde_json::from_slice(bytes)
        .map_err(|err| BusError::Store(format!("invalid revision-range evidence: {err}")))?;
    if stored.schema_v != 2
        || stored.base.reference.is_empty()
        || stored.head.reference.is_empty()
        || stored
            .head
            .content_revision
            .as_deref()
            .is_none_or(str::is_empty)
        || !valid_commit_id(&stored.base.commit)
        || !valid_commit_id(&stored.head.commit)
        || !valid_commit_id(&stored.merge_base)
    {
        return Err(BusError::Store(
            "revision-range evidence has invalid exact provenance".into(),
        ));
    }
    Ok(RevisionRange {
        base_ref: stored.base.reference,
        base_commit: stored.base.commit,
        head_ref: stored.head.reference,
        head_commit: stored.head.commit,
        head_content_revision: stored.head.content_revision.unwrap_or_default(),
        merge_base: stored.merge_base,
    })
}

fn valid_commit_id(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn merge_browser_proof_evidence(
    evidence: &mut BTreeMap<String, BrowserProofEvidence>,
    bytes: &[u8],
) -> Result<(), BusError> {
    let stored: StoredBrowserProgramEvidence = serde_json::from_slice(bytes)
        .map_err(|err| BusError::Store(format!("invalid browser program evidence: {err}")))?;
    if stored.schema_v != 2 {
        return Err(BusError::Store(format!(
            "unknown browser program evidence schema {}",
            stored.schema_v
        )));
    }
    if stored.program.is_empty() {
        return Err(BusError::Store(
            "browser program evidence omitted program identity".into(),
        ));
    }
    let expected_asserted = stored
        .assertions
        .iter()
        .filter(|assertion| assertion.status == "passed")
        .map(|assertion| assertion.obligation.clone())
        .collect::<BTreeSet<_>>();
    let expected_contradicted = stored
        .assertions
        .iter()
        .filter(|assertion| assertion.status == "contradicted")
        .map(|assertion| assertion.obligation.clone())
        .collect::<BTreeSet<_>>();
    if expected_asserted != stored.asserted.iter().cloned().collect()
        || expected_contradicted != stored.contradicted.iter().cloned().collect()
    {
        return Err(BusError::Store(format!(
            "browser program {} aggregate assertion lists do not match exact evidence",
            stored.program
        )));
    }
    for assertion in &stored.assertions {
        if assertion.obligation.is_empty()
            || !matches!(
                assertion.status.as_str(),
                "passed" | "contradicted" | "failed"
            )
            || assertion
                .observation
                .as_ref()
                .is_some_and(|observation| !stored.observations.contains(observation))
        {
            return Err(BusError::Store(format!(
                "browser program {} has invalid exact assertion evidence",
                stored.program
            )));
        }
        let entry = evidence.entry(assertion.obligation.clone()).or_default();
        entry.programs.insert(stored.program.clone());
        for kind in &stored.present {
            if !entry.present.contains(kind) {
                entry.present.push(*kind);
            }
        }
        if let Some(observation) = &assertion.observation
            && !entry.observations.contains(observation)
        {
            entry.observations.push(observation.clone());
        }
        match assertion.status.as_str() {
            "passed" => entry.passed = true,
            "failed" => entry.failed = true,
            "contradicted" => entry.contradicted = true,
            _ => unreachable!("validated browser assertion status"),
        }
    }
    for observation in &stored.observations {
        if observation.is_empty() {
            return Err(BusError::Store(format!(
                "browser program {} has an empty observation handle",
                stored.program
            )));
        }
    }
    Ok(())
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
    repo: &Path,
    revision: &RevisionId,
    graph: &Value,
    records: &[ExecutorRecord],
    bindings: &[TestBinding],
) -> Result<Option<wvq_proof::ProtectionSnapshot>, BusError> {
    let Some(nodes) = graph.get("nodes").and_then(Value::as_array) else {
        return Ok(None);
    };
    if nodes.is_empty() {
        return Ok(None);
    }
    let executed_tests = executed_test_inventory(repo, records, bindings)?;
    let (flows, coverage_files) =
        measured_protection_flows(repo, revision, graph, nodes, records, bindings)?;
    if flows.is_empty() {
        if !coverage_files.is_empty() {
            return Err(coverage_graph_mismatch(nodes, coverage_files));
        }
        return Ok(None);
    }
    let snapshot =
        snapshot_with_executed_tests(revision, flows.into_values().collect(), executed_tests)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
    Ok(Some(snapshot))
}

/// Record every exact passing case independently of the flows it covered.
///
/// Coverage attribution remains deliberately stricter: a batch artifact may
/// only protect at executor scope. The inventory has a different job — proving
/// that a named case still executed, even when it reached no impacted symbol.
fn executed_test_inventory(
    repo: &Path,
    records: &[ExecutorRecord],
    bindings: &[TestBinding],
) -> Result<Vec<String>, BusError> {
    let mut identities = BTreeSet::new();
    for record in records.iter().filter(|record| record.passed) {
        for artifact in record
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == "normalized-test-run")
        {
            let normalized: NormalizedTestRun =
                serde_json::from_slice(&artifact.bytes).map_err(|err| {
                    BusError::Runtime(format!(
                        "cannot decode normalized evidence from {}: {err}",
                        artifact.path
                    ))
                })?;
            for case in normalized
                .cases
                .into_iter()
                .filter(|case| case.status == TestStatus::Pass)
            {
                let matched = bindings
                    .iter()
                    .filter(|binding| {
                        binding.case.as_deref() == Some(case.name.as_str())
                            && binding
                                .runner
                                .as_deref()
                                .is_none_or(|runner| runner == record.executor)
                            && normalized_suite_matches(repo, record, binding, &case.suite)
                    })
                    .map(|binding| format!("{}#{}", binding.path, case.name))
                    .collect::<Vec<_>>();
                if matched.is_empty() {
                    identities.insert(format!("{}:{}#{}", record.executor, case.suite, case.name));
                } else {
                    identities.extend(matched);
                }
            }
        }
    }
    Ok(identities.into_iter().collect())
}

fn persist_dynamic_coverage_history(
    store: &Store,
    run: &RunId,
    revision: &RevisionId,
    graph: &Value,
    records: &[ExecutorRecord],
) -> Result<(), BusError> {
    let mut observations = BTreeMap::<String, BTreeSet<String>>::new();
    for record in records
        .iter()
        .filter(|record| record.passed && record.selection.len() == 1)
    {
        let test = &record.selection[0];
        if !is_test_path(test) {
            continue;
        }
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
            let mapped = map_coverage_to_nodes(Some(&coverage), graph)
                .map_err(|err| BusError::Intelligence(err.to_string()))?;
            observations.entry(test.clone()).or_default().extend(
                mapped
                    .into_iter()
                    .filter(|node| node.measurement == CoverageMeasurement::Covered)
                    .map(|node| node.node_id),
            );
        }
    }
    for (test, nodes) in observations {
        store
            .observe_test_nodes(run, &test, &nodes.into_iter().collect::<Vec<_>>(), revision)
            .map_err(|err| BusError::Store(err.to_string()))?;
    }
    Ok(())
}

fn measured_protection_flows(
    repo: &Path,
    revision: &RevisionId,
    graph: &Value,
    nodes: &[Value],
    records: &[ExecutorRecord],
    bindings: &[TestBinding],
) -> Result<(BTreeMap<String, FlowProtection>, BTreeSet<String>), BusError> {
    let mut known_nodes = BTreeSet::new();
    let symbol_files = nodes
        .iter()
        .filter(|node| node.get("kind").and_then(Value::as_str) != Some("file"))
        .filter_map(|node| node.pointer("/span/file").and_then(Value::as_str))
        .map(normalize_path)
        .collect::<BTreeSet<_>>();
    for node in nodes {
        let Some(id) = graph_node_id(node) else {
            return Err(BusError::Intelligence(
                "impacted head graph contains a node without identity".into(),
            ));
        };
        known_nodes.insert(id);
    }

    let mut flows = BTreeMap::<String, FlowProtection>::new();
    let mut coverage_files = BTreeSet::new();
    for record in records.iter().filter(|record| record.passed) {
        let protectors = coverage_protectors(repo, record, bindings)?;
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
            coverage_files.extend(coverage.files.iter().map(|file| file.path.clone()));
            let mapped = map_coverage_to_nodes(Some(&coverage), graph)
                .map_err(|err| BusError::Intelligence(err.to_string()))?;
            for node in mapped
                .into_iter()
                .filter(|node| node.measurement != CoverageMeasurement::Unmeasured)
            {
                let node_id = node.node_id;
                if node_id
                    .strip_prefix("file:")
                    .is_some_and(|path| symbol_files.contains(&normalize_path(path)))
                {
                    // A file node is only a fallback when Weavatrix has no
                    // symbol span for that source. Keeping both lets one hit in
                    // ViewerLabel hide an uncovered CanDelete function.
                    continue;
                }
                if !known_nodes.contains(&node_id) {
                    return Err(BusError::Intelligence(format!(
                        "coverage mapped unknown graph node {node_id}"
                    )));
                }
                let flow = flows
                    .entry(node_id.clone())
                    .or_insert_with(|| FlowProtection {
                        flow: node_id.clone(),
                        revision: revision.to_string(),
                        tests: Vec::new(),
                        sessions: Vec::new(),
                        covered_nodes: Vec::new(),
                        covered_branches: Vec::new(),
                        proven_obligations: Vec::new(),
                        proofs: Vec::new(),
                    });
                if node.measurement != CoverageMeasurement::Covered {
                    continue;
                }
                for protector in &protectors {
                    if !flow.tests.contains(&protector.identity) {
                        flow.tests.push(protector.identity.clone());
                    }
                    for obligation in &protector.obligations {
                        if !flow.proven_obligations.contains(obligation) {
                            flow.proven_obligations.push(obligation.clone());
                        }
                    }
                }
                if !flow.covered_nodes.contains(&node_id) {
                    flow.covered_nodes.push(node_id);
                }
            }
        }
    }
    Ok((flows, coverage_files))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CoverageProtector {
    identity: String,
    obligations: Vec<String>,
}

/// Attribute one coverage artifact to an exact test only when the invocation
/// makes that attribution unambiguous. A batch-wide coverage file remains
/// executor-level evidence: guessing which case reached a flow would turn a
/// test list into proof.
fn coverage_protectors(
    repo: &Path,
    record: &ExecutorRecord,
    bindings: &[TestBinding],
) -> Result<Vec<CoverageProtector>, BusError> {
    let mut cases = Vec::new();
    for artifact in record
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == "normalized-test-run")
    {
        let normalized: NormalizedTestRun =
            serde_json::from_slice(&artifact.bytes).map_err(|err| {
                BusError::Runtime(format!(
                    "cannot decode normalized evidence from {}: {err}",
                    artifact.path
                ))
            })?;
        cases.extend(
            normalized
                .cases
                .into_iter()
                .filter(|case| case.status == TestStatus::Pass),
        );
    }
    if cases.len() == 1 {
        let case = &cases[0];
        let matched = bindings
            .iter()
            .filter(|binding| {
                binding.case.as_deref() == Some(case.name.as_str())
                    && binding
                        .runner
                        .as_deref()
                        .is_none_or(|runner| runner == record.executor)
                    && normalized_suite_matches(repo, record, binding, &case.suite)
            })
            .collect::<Vec<_>>();
        if !matched.is_empty() {
            let mut by_identity = BTreeMap::<String, BTreeSet<String>>::new();
            for binding in matched {
                by_identity
                    .entry(format!("{}#{}", binding.path, case.name))
                    .or_default()
                    .extend(binding.obligations.iter().cloned());
            }
            return Ok(by_identity
                .into_iter()
                .map(|(identity, obligations)| CoverageProtector {
                    identity,
                    obligations: obligations.into_iter().collect(),
                })
                .collect());
        }
        return Ok(vec![CoverageProtector {
            identity: format!("{}:{}#{}", record.executor, case.suite, case.name),
            obligations: Vec::new(),
        }]);
    }
    if record.selection.len() == 1 {
        let identity = record.selection[0].clone();
        let obligations = bindings
            .iter()
            .filter(|binding| binding.case.is_none() && binding.path == identity)
            .flat_map(|binding| binding.obligations.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        return Ok(vec![CoverageProtector {
            identity,
            obligations,
        }]);
    }
    Ok(vec![CoverageProtector {
        identity: format!("executor:{}@{}", record.executor, record.cwd),
        obligations: Vec::new(),
    }])
}

fn coverage_graph_mismatch(nodes: &[Value], coverage_files: BTreeSet<String>) -> BusError {
    let graph_files = nodes
        .iter()
        .filter_map(|node| {
            node.pointer("/span/file")
                .or_else(|| node.get("file"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect::<BTreeSet<_>>();
    let graph_spans = nodes
        .iter()
        .filter_map(|node| {
            Some(format!(
                "{}:{}-{}",
                node.pointer("/span/file")
                    .or_else(|| node.get("file"))?
                    .as_str()?,
                node.pointer("/span/start_line")
                    .or_else(|| node.pointer("/span/start/line"))
                    .or_else(|| node.get("start_line"))?
                    .as_u64()?,
                node.pointer("/span/end_line")
                    .or_else(|| node.pointer("/span/end/line"))
                    .or_else(|| node.get("end_line"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            ))
        })
        .take(20)
        .collect::<Vec<_>>();
    let graph_sample = nodes.iter().take(3).cloned().collect::<Vec<_>>();
    BusError::Intelligence(format!(
        "coverage files [{}] do not overlap measured protection spans [{}] in graph files [{}]; graph sample {}",
        coverage_files.into_iter().collect::<Vec<_>>().join(", "),
        graph_spans.join(", "),
        graph_files.into_iter().collect::<Vec<_>>().join(", "),
        Value::Array(graph_sample)
    ))
}

fn expectation_change(
    base: &[TestObligation],
    head: &[TestObligation],
    seal_changed: bool,
) -> (Vec<String>, Vec<(String, String)>) {
    let mut changed = BTreeSet::new();
    for before in base {
        if let Some(after) = head.iter().find(|item| item.id == before.id)
            && before != after
        {
            changed.insert(before.id.to_string());
        }
    }
    let removed = base
        .iter()
        .filter(|before| !head.iter().any(|after| after.id == before.id))
        .collect::<Vec<_>>();
    let added = head
        .iter()
        .filter(|after| !base.iter().any(|before| before.id == after.id))
        .collect::<Vec<_>>();
    let same_slot = |before: &TestObligation, after: &TestObligation| {
        before.requirement == after.requirement
            && before.scenario == after.scenario
            && before.kind == after.kind
    };
    let mut replacements = Vec::new();
    for before in &removed {
        changed.insert(before.id.to_string());
        let candidates = added
            .iter()
            .filter(|after| same_slot(before, after))
            .collect::<Vec<_>>();
        if candidates.len() == 1
            && removed
                .iter()
                .filter(|candidate| same_slot(candidate, candidates[0]))
                .count()
                == 1
        {
            replacements.push((before.id.to_string(), candidates[0].id.to_string()));
        }
    }
    changed.extend(added.iter().map(|item| item.id.to_string()));
    if seal_changed && changed.is_empty() {
        changed.extend(base.iter().map(|item| item.id.to_string()));
        changed.extend(head.iter().map(|item| item.id.to_string()));
    }
    replacements.sort();
    replacements.dedup();
    (changed.into_iter().collect(), replacements)
}

fn build_protection_view(
    obligations: &[TestObligation],
    diff: &Value,
    snapshots: (&ProtectionSnapshot, &ProtectionSnapshot),
    graphs: (&Value, &Value),
    files: &ChangedFiles,
    oracle_replacement: Option<OracleReplacementReview>,
) -> ProtectionView {
    let (base, head) = snapshots;
    let (base_graph, head_graph) = graphs;
    let mut relocations = graph_relocations(diff);
    relocations.extend(snapshot_relocations(base, head));
    relocations.sort();
    relocations.dedup();
    let context = DeltaContext {
        critical_branches: Vec::new(),
        intentionally_removed: Vec::new(),
        approved_replaced_flows: approved_replaced_flows(base, head, oracle_replacement.as_ref()),
        relocations,
        changed_obligations: oracle_replacement
            .as_ref()
            .map(|review| review.changed_obligations.clone())
            .unwrap_or_default(),
        obligation_replacements: oracle_replacement
            .as_ref()
            .map(|review| review.obligation_replacements.clone())
            .unwrap_or_default(),
        oracle_replacement_approved: oracle_replacement
            .as_ref()
            .is_some_and(|review| review.approved),
    };
    let deltas = protection_delta(base, head, &context);
    let lineage = protection_lineage(base, head);
    let changed_tests = files.changed_tests().into_iter().collect::<BTreeSet<_>>();
    let tests = protection_test_changes(base, head, &deltas, &changed_tests, &context);
    let any_high_risk = obligations
        .iter()
        .any(|item| matches!(item.risk, RiskLevel::High | RiskLevel::Critical));
    let high_risk_flows = if any_high_risk {
        deltas.iter().map(|item| item.flow.clone()).collect()
    } else {
        Vec::new()
    };
    let findings = gate_protection(&ProtectionCheckInput {
        deltas: deltas.clone(),
        tests,
        trends: Vec::new(),
        policy: ProtectionPolicy {
            high_risk_flows,
            substitution_ratio: 10,
        },
    });
    let flows = deltas
        .iter()
        .map(|delta| {
            let before = base.flow(&delta.flow);
            let head_name = context
                .relocations
                .iter()
                .find(|(source, _)| source == &delta.flow)
                .map_or(delta.flow.as_str(), |(_, target)| target.as_str());
            let after = head.flow(head_name);
            FlowView {
                flow: delta.flow.clone(),
                base_path: graph_singleton_path(base_graph, &delta.flow),
                head_path: graph_singleton_path(head_graph, head_name),
                requirements: before
                    .into_iter()
                    .flat_map(|item| item.proven_obligations.iter().cloned())
                    .chain(
                        after
                            .into_iter()
                            .flat_map(|item| item.proven_obligations.iter().cloned()),
                    )
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                tests_before: before.map(|item| item.tests.clone()).unwrap_or_default(),
                tests_after: after.map(|item| item.tests.clone()).unwrap_or_default(),
                coverage_before: before
                    .map(|item| item.covered_branches.clone())
                    .unwrap_or_default(),
                coverage_after: after
                    .map(|item| item.covered_branches.clone())
                    .unwrap_or_default(),
                proof_before: before.map(|item| item.proofs.clone()).unwrap_or_default(),
                proof_after: after.map(|item| item.proofs.clone()).unwrap_or_default(),
            }
        })
        .collect();
    ProtectionView {
        deltas,
        findings,
        lineage,
        flows,
        oracle_replacement,
    }
}

fn protection_test_changes(
    base: &ProtectionSnapshot,
    head: &ProtectionSnapshot,
    deltas: &[ProtectionDelta],
    changed_tests: &BTreeSet<String>,
    context: &DeltaContext,
) -> Vec<TestChange> {
    let mut changes = Vec::new();
    for item in protection_lineage(base, head) {
        for flow in &item.lost_flows {
            let delta = deltas.iter().find(|delta| delta.flow == *flow);
            if delta.is_some_and(|delta| {
                matches!(
                    delta.state,
                    ProtectionDeltaState::Preserved
                        | ProtectionDeltaState::Improved
                        | ProtectionDeltaState::Replaced
                        | ProtectionDeltaState::Relocated
                )
            }) {
                // Source identity changed, but measured protection followed the
                // semantic flow. Moving a test with its implementation is not
                // evidence that its oracle was weakened.
                continue;
            }
            changes.push(TestChange {
                test: item.test.clone(),
                flow: flow.clone(),
                survives: item.state != "removed",
                lost_flows: item.lost_flows.clone(),
                lost_obligations: delta
                    .map(|delta| delta.lost_obligations.clone())
                    .unwrap_or_default(),
                replaced_by: replacement_test_for_flow(base, head, flow, context),
                assertions_weakened: false,
                changed_with_implementation: changed_tests
                    .iter()
                    .any(|path| test_identity_has_path(&item.test, path)),
                new_oracle_seal: context.oracle_replacement_approved,
                declared_spec_delta: !context.changed_obligations.is_empty(),
            });
        }
    }
    changes
}

fn approved_replaced_flows(
    base: &ProtectionSnapshot,
    head: &ProtectionSnapshot,
    review: Option<&OracleReplacementReview>,
) -> Vec<String> {
    let Some(review) = review.filter(|review| review.approved) else {
        return Vec::new();
    };
    let proven_on_head = head
        .flows
        .iter()
        .flat_map(|flow| flow.proven_obligations.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    base.flows
        .iter()
        .filter(|flow| {
            head.flow(&flow.flow)
                .is_none_or(|candidate| !candidate.is_protected())
        })
        .filter(|flow| {
            !flow.proven_obligations.is_empty()
                && flow.proven_obligations.iter().all(|before| {
                    review
                        .obligation_replacements
                        .iter()
                        .any(|(from, to)| from == before && proven_on_head.contains(to.as_str()))
                })
        })
        .map(|flow| flow.flow.clone())
        .collect()
}

fn replacement_test_for_flow(
    base: &ProtectionSnapshot,
    head: &ProtectionSnapshot,
    flow: &str,
    context: &DeltaContext,
) -> Option<String> {
    if !context.oracle_replacement_approved {
        return None;
    }
    let before = base.flow(flow)?;
    let targets = before
        .proven_obligations
        .iter()
        .filter_map(|obligation| {
            context
                .obligation_replacements
                .iter()
                .find(|(from, _)| from == obligation)
                .map(|(_, to)| to.as_str())
        })
        .collect::<BTreeSet<_>>();
    if targets.len() != before.proven_obligations.len() {
        return None;
    }
    head.flows
        .iter()
        .filter(|candidate| {
            candidate
                .proven_obligations
                .iter()
                .any(|obligation| targets.contains(obligation.as_str()))
        })
        .flat_map(|candidate| candidate.tests.iter())
        .min()
        .cloned()
}

fn test_identity_has_path(identity: &str, path: &str) -> bool {
    identity == path
        || identity
            .strip_prefix(path)
            .is_some_and(|suffix| suffix.starts_with('#'))
}

fn graph_relocations(diff: &Value) -> Vec<(String, String)> {
    let mut relocations = values_at(diff, "/nodes/changed")
        .iter()
        .filter_map(|changed| {
            let before = changed.get("before").and_then(graph_node_id)?;
            let after = changed.get("after").and_then(graph_node_id)?;
            (before != after).then_some((before, after))
        })
        .collect::<Vec<_>>();
    let removed = values_at(diff, "/nodes/removed")
        .iter()
        .filter_map(graph_node_id)
        .collect::<Vec<_>>();
    let added = values_at(diff, "/nodes/added")
        .iter()
        .filter_map(graph_node_id)
        .collect::<Vec<_>>();
    for before in &removed {
        let Some(signature) = stable_symbol_signature(before) else {
            continue;
        };
        let candidates = added
            .iter()
            .filter(|after| stable_symbol_signature(after) == Some(signature))
            .collect::<Vec<_>>();
        if candidates.len() == 1
            && removed
                .iter()
                .filter(|candidate| stable_symbol_signature(candidate) == Some(signature))
                .count()
                == 1
        {
            relocations.push((before.clone(), candidates[0].clone()));
        }
    }
    relocations.sort();
    relocations.dedup();
    relocations
}

fn snapshot_relocations(
    base: &ProtectionSnapshot,
    head: &ProtectionSnapshot,
) -> Vec<(String, String)> {
    let mut relocations = Vec::new();
    for before in &base.flows {
        if head.flow(&before.flow).is_some() {
            continue;
        }
        let Some(signature) = stable_symbol_signature(&before.flow) else {
            continue;
        };
        let candidates = head
            .flows
            .iter()
            .filter(|after| stable_symbol_signature(&after.flow) == Some(signature))
            .collect::<Vec<_>>();
        if candidates.len() == 1
            && base
                .flows
                .iter()
                .filter(|candidate| stable_symbol_signature(&candidate.flow) == Some(signature))
                .count()
                == 1
        {
            relocations.push((before.flow.clone(), candidates[0].flow.clone()));
        }
    }
    relocations.sort();
    relocations.dedup();
    relocations
}

fn stable_symbol_signature(id: &str) -> Option<&str> {
    let tail = id.strip_prefix("symbol:")?.split_once('#')?.1;
    let Some((symbol, position)) = tail.rsplit_once('@') else {
        return Some(tail);
    };
    let is_source_position = position.split_once(':').is_some_and(|(line, column)| {
        !line.is_empty()
            && !column.is_empty()
            && line.bytes().all(|byte| byte.is_ascii_digit())
            && column.bytes().all(|byte| byte.is_ascii_digit())
    });
    Some(if is_source_position { symbol } else { tail })
}

fn protection_lineage(
    base: &ProtectionSnapshot,
    head: &ProtectionSnapshot,
) -> Vec<TestLineageView> {
    let mut base_flows = BTreeMap::<String, BTreeSet<String>>::new();
    let mut head_flows = BTreeMap::<String, BTreeSet<String>>::new();
    for flow in &base.flows {
        for test in &flow.tests {
            base_flows
                .entry(test.clone())
                .or_default()
                .insert(flow.flow.clone());
        }
    }
    for flow in &head.flows {
        for test in &flow.tests {
            head_flows
                .entry(test.clone())
                .or_default()
                .insert(flow.flow.clone());
        }
    }
    let base_executed = base.executed_test_identities();
    let head_executed = head.executed_test_identities();
    let tests = base_executed
        .iter()
        .chain(&head_executed)
        .cloned()
        .collect::<BTreeSet<_>>();
    tests
        .into_iter()
        .map(|test| {
            let before = base_flows.get(&test).cloned().unwrap_or_default();
            let after = head_flows.get(&test).cloned().unwrap_or_default();
            let lost_flows = before.difference(&after).cloned().collect::<Vec<_>>();
            let gained_flows = after.difference(&before).cloned().collect::<Vec<_>>();
            let present_before = base_executed.contains(&test);
            let present_after = head_executed.contains(&test);
            let phantom = present_before && present_after && !lost_flows.is_empty();
            TestLineageView {
                state: match (present_before, present_after) {
                    (true, true) => "unchanged",
                    (true, false) => "removed",
                    (false, true) => "added",
                    (false, false) => "unknown",
                }
                .into(),
                matched_on: "exact test identity".into(),
                protection_changed: !lost_flows.is_empty() || !gained_flows.is_empty(),
                phantom,
                test,
                lost_flows,
                gained_flows,
            }
        })
        .collect()
}

fn graph_singleton_path(graph: &Value, flow: &str) -> Vec<String> {
    if graph
        .get("nodes")
        .and_then(Value::as_array)
        .is_some_and(|nodes| {
            nodes
                .iter()
                .any(|node| graph_node_id(node).as_deref() == Some(flow))
        })
    {
        vec![flow.to_owned()]
    } else {
        Vec::new()
    }
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

fn graph_node_is_public_function(node: &Value) -> bool {
    let kind = node
        .get("kind")
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase);
    matches!(kind.as_deref(), Some("function" | "method"))
        && node
            .pointer("/attributes/exported")
            .and_then(Value::as_bool)
            == Some(true)
        && node
            .pointer("/attributes/test_only")
            .and_then(Value::as_bool)
            != Some(true)
        && graph_node_source_path(node).is_none_or(|path| !is_test_path(path))
}

fn graph_node_source_path(node: &Value) -> Option<&str> {
    node.get("path").and_then(Value::as_str).or_else(|| {
        node.get("id")
            .and_then(Value::as_str)?
            .strip_prefix("symbol:")?
            .split_once('#')
            .map(|(path, _)| path)
    })
}

fn recovery_public_symbol_id(node: &Value) -> Option<String> {
    let id = graph_node_id(node)?;
    Some(
        id.rsplit_once('@')
            .map_or(id.clone(), |(stable, _)| stable.to_owned()),
    )
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

#[derive(Debug)]
struct PersistedTestAnalytics {
    recorded_test_count: u64,
    failed_test_count: u64,
    flaky_test_count: u64,
    unknown_failure_count: u64,
    bytes: Vec<u8>,
}

#[derive(serde::Serialize)]
struct TestAnalyticsDocument {
    schema_v: u32,
    run_id: String,
    revision: String,
    recorded_cases: u64,
    outcomes: TestOutcomeCounts,
    failure_occurrences: Vec<Value>,
    flaky_tests: Vec<Value>,
    slowest_tests: Vec<Value>,
    runtime_llm_tokens: u64,
}

#[derive(serde::Serialize)]
struct TestOutcomeCounts {
    passed: u64,
    failed: u64,
    errors: u64,
    skipped: u64,
}

#[derive(Debug)]
struct ObservedTestCase {
    executor: String,
    suite: String,
    name: String,
    status: TestStatus,
    duration_ms: Option<u64>,
    message: Option<String>,
}

fn collect_observed_test_cases(
    records: &[ExecutorRecord],
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
) -> Result<Vec<ObservedTestCase>, BusError> {
    let mut observed = Vec::new();
    for record in records {
        for artifact in record
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == "normalized-test-run")
        {
            let normalized: NormalizedTestRun =
                serde_json::from_slice(&artifact.bytes).map_err(|err| {
                    BusError::Runtime(format!(
                        "cannot decode {} from {}: {err}",
                        artifact.kind, artifact.path
                    ))
                })?;
            observed.extend(normalized.cases.into_iter().map(|case| ObservedTestCase {
                executor: record.executor.clone(),
                suite: case.suite,
                name: case.name,
                status: case.status,
                duration_ms: case.duration_ms,
                message: case.message,
            }));
        }
        if let Some(error) = &record.error {
            observed.push(ObservedTestCase {
                executor: record.executor.clone(),
                suite: record.cwd.clone(),
                name: "<executor invocation>".into(),
                status: TestStatus::Error,
                duration_ms: None,
                message: Some(error.clone()),
            });
        }
    }
    observed.extend(
        browser_runs
            .iter()
            .map(|(configured, result)| ObservedTestCase {
                executor: "playwright-browser".into(),
                suite: configured.path.clone(),
                name: result.program.clone(),
                status: if result.passed {
                    TestStatus::Pass
                } else {
                    TestStatus::Fail
                },
                duration_ms: None,
                message: result.failure.clone(),
            }),
    );
    Ok(observed)
}

fn persist_failure(
    store: &Store,
    run: &RunId,
    index: usize,
    case: &ObservedTestCase,
    status: &str,
) -> Result<(wvq_domain::ContentHash, FlakeClass, u64), BusError> {
    let message = case.message.as_deref().unwrap_or(status);
    let evidence = FailureEvidence {
        program: format!("{}::{}", case.suite, case.name),
        executor: case.executor.clone(),
        stack_digest: Some(sha256_hex(message.as_bytes())),
        timing_bucket: failure_timing_bucket(message),
        ..FailureEvidence::default()
    };
    let digest = fingerprint_id(&evidence).map_err(|err| BusError::Runtime(err.to_string()))?;
    let previous = store
        .failure_cluster_size(&digest)
        .map_err(|err| BusError::Store(err.to_string()))?;
    let classification = triage(&evidence, previous > 0).class;
    store
        .put_failure_fingerprint(&digest, flake_class_token(classification))
        .map_err(|err| BusError::Store(err.to_string()))?;
    store
        .put_failure_occurrence(&format!("{}-failure-{index}", run.as_str()), &digest)
        .map_err(|err| BusError::Store(err.to_string()))?;
    Ok((digest, classification, previous))
}

#[allow(clippy::too_many_lines)]
fn persist_test_analytics(
    store: &Store,
    run: &RunId,
    revision: &RevisionId,
    records: &[ExecutorRecord],
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
) -> Result<PersistedTestAnalytics, BusError> {
    let observed = collect_observed_test_cases(records, browser_runs)?;

    let mut passed = 0_u64;
    let mut failed = 0_u64;
    let mut errors = 0_u64;
    let mut skipped = 0_u64;
    let mut unknown_failures = 0_u64;
    let mut failures = Vec::new();
    let mut flaky = BTreeMap::<(String, String, String), Value>::new();
    let mut durations = Vec::<(u64, Value)>::new();

    for (index, case) in observed.iter().enumerate() {
        match case.status {
            TestStatus::Pass => passed = passed.saturating_add(1),
            TestStatus::Fail => failed = failed.saturating_add(1),
            TestStatus::Error => errors = errors.saturating_add(1),
            TestStatus::Skip => skipped = skipped.saturating_add(1),
        }
        let status = test_status_token(case.status);
        let failure = if matches!(case.status, TestStatus::Fail | TestStatus::Error) {
            let (digest, classification, previous) =
                persist_failure(store, run, index, case, status)?;
            if classification == FlakeClass::Unknown {
                unknown_failures = unknown_failures.saturating_add(1);
            }
            failures.push(json!({
                "executor": case.executor,
                "suite": case.suite,
                "name": case.name,
                "status": status,
                "fingerprint": digest.as_str(),
                "classification": flake_class_token(classification),
                "previous_occurrences": previous,
            }));
            Some(digest)
        } else {
            None
        };
        store
            .put_test_case_result(&StoredTestCaseResult {
                id: format!("{}-test-{index}", run.as_str()),
                run_id: run.clone(),
                revision: revision.clone(),
                executor: case.executor.clone(),
                suite: case.suite.clone(),
                name: case.name.clone(),
                status: status.into(),
                duration_ms: case.duration_ms,
                fingerprint: failure,
            })
            .map_err(|err| BusError::Store(err.to_string()))?;
        let history = store
            .test_case_stats(&case.executor, &case.suite, &case.name)
            .map_err(|err| BusError::Store(err.to_string()))?;
        let identity = (case.executor.clone(), case.suite.clone(), case.name.clone());
        if history.flaky {
            flaky.insert(
                identity.clone(),
                json!({
                    "executor": case.executor,
                    "suite": case.suite,
                    "name": case.name,
                    "runs": history.runs,
                    "passes": history.passes,
                    "failures": history.failures,
                    "errors": history.errors,
                }),
            );
        }
        if let Some(duration_ms) = case.duration_ms {
            durations.push((
                duration_ms,
                json!({
                    "executor": case.executor,
                    "suite": case.suite,
                    "name": case.name,
                    "duration_ms": duration_ms,
                    "historical_average_ms": history.average_duration_ms,
                }),
            ));
        }
    }
    durations.sort_by(|left, right| right.0.cmp(&left.0));
    durations.truncate(20);
    let recorded = u64::try_from(observed.len()).unwrap_or(u64::MAX);
    let flaky_values = flaky.into_values().collect::<Vec<_>>();
    let flaky_count = u64::try_from(flaky_values.len()).unwrap_or(u64::MAX);
    let bytes = serde_json::to_vec_pretty(&TestAnalyticsDocument {
        schema_v: 1,
        run_id: run.to_string(),
        revision: revision.to_string(),
        recorded_cases: recorded,
        outcomes: TestOutcomeCounts {
            passed,
            failed,
            errors,
            skipped,
        },
        failure_occurrences: failures,
        flaky_tests: flaky_values,
        slowest_tests: durations.into_iter().map(|(_, value)| value).collect(),
        runtime_llm_tokens: 0,
    })
    .map_err(|err| BusError::Runtime(format!("cannot encode test analytics: {err}")))?;
    Ok(PersistedTestAnalytics {
        recorded_test_count: recorded,
        failed_test_count: failed.saturating_add(errors),
        flaky_test_count: flaky_count,
        unknown_failure_count: unknown_failures,
        bytes,
    })
}

fn test_status_token(status: TestStatus) -> &'static str {
    match status {
        TestStatus::Pass => "pass",
        TestStatus::Fail => "fail",
        TestStatus::Skip => "skip",
        TestStatus::Error => "error",
    }
}

fn failure_timing_bucket(message: &str) -> Option<TimingBucket> {
    let message = message.to_ascii_lowercase();
    (message.contains("timed out")
        || message.contains("timeout")
        || message.contains("deadline exceeded"))
    .then_some(TimingBucket::Timeout)
}

fn flake_class_token(class: FlakeClass) -> &'static str {
    match class {
        FlakeClass::Known => "known",
        FlakeClass::ProductRegression => "product_regression",
        FlakeClass::Ordering => "ordering",
        FlakeClass::Timing => "timing",
        FlakeClass::Network => "network",
        FlakeClass::Environment => "environment",
        FlakeClass::SelectorDrift => "selector_drift",
        FlakeClass::Seed => "seed",
        FlakeClass::TestOrder => "test_order",
        FlakeClass::Unknown => "unknown",
    }
}

#[allow(clippy::too_many_arguments)]
fn execution_summary(
    run: &RunId,
    change: &str,
    revision: &RevisionId,
    range: &RevisionRange,
    requested_scope: &str,
    effective_scope: &str,
    scope_reason: &str,
    evidence_policy: &str,
    outcome: &str,
    records: &[ExecutorRecord],
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
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
    let browser_items = browser_runs
        .iter()
        .map(|(configured, result)| {
            json!({
                "program": result.program,
                "path": configured.path,
                "passed": result.passed,
                "asserted": result.asserted,
                "contradicted": result.contradicted,
                "observations": result.observations.len(),
                "screenshots": result.screenshot_paths.len(),
                "trace": result.trace_path.is_some(),
                "failure": result.failure,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_vec_pretty(&json!({
        "schema_v": 1,
        "run_id": run.as_str(),
        "change": change,
        "revision": revision.as_str(),
        "base": {"ref": range.base_ref, "commit": range.base_commit},
        "head": {"ref": range.head_ref, "commit": range.head_commit},
        "merge_base": range.merge_base,
        "requested_scope": requested_scope,
        "effective_scope": effective_scope,
        "scope_reason": scope_reason,
        "evidence_policy": evidence_policy,
        "outcome": outcome,
        "executors": items,
        "browser_programs": browser_items,
    }))
    .map_err(|err| BusError::Runtime(format!("cannot encode execution summary: {err}")))
}

const MAX_RUNNER_ARTIFACT_BYTES: u64 = 8 * 1024 * 1024;
const ARTIFACT_CLOCK_TOLERANCE: Duration = Duration::from_secs(2);

fn clear_generated_runner_artifacts(cwd: &Path) -> Result<(), BusError> {
    for relative in [
        ".weavatrix-quality/junit.xml",
        ".weavatrix-quality/go-cover.out",
    ] {
        let path = cwd.join(relative);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(BusError::Runtime(format!(
                    "cannot clear generated runner artifact {}: {err}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn attach_normalized_artifacts(
    repo: &Path,
    cwd: &Path,
    started: SystemTime,
    record: &mut ExecutorRecord,
) {
    if record.executor == "cargo-test" && (!record.stdout.is_empty() || !record.stderr.is_empty()) {
        match std::str::from_utf8(&record.stdout)
            .map_err(|err| format!("cargo-test stdout is not UTF-8: {err}"))
            .and_then(|stdout| {
                std::str::from_utf8(&record.stderr)
                    .map_err(|err| format!("cargo-test stderr is not UTF-8: {err}"))
                    .map(|stderr| (stdout, stderr))
            })
            .and_then(|(stdout, stderr)| {
                parse_cargo_test(stdout, stderr).map_err(|err| err.to_string())
            })
            .and_then(|run| {
                serde_json::to_vec_pretty(&run)
                    .map_err(|err| format!("cannot encode normalized cargo-test: {err}"))
            }) {
            Ok(bytes) => record.artifacts.push(ProducedArtifact {
                kind: "normalized-test-run".into(),
                path: "cargo-test#normalized".into(),
                bytes,
            }),
            Err(err) => set_record_error(record, err),
        }
    }

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
                    let failed_cases = run
                        .cases
                        .iter()
                        .filter(|case| matches!(case.status, TestStatus::Fail | TestStatus::Error))
                        .count();
                    serde_json::to_vec_pretty(&run)
                        .map_err(|err| wvq_runtime::RuntimeError::Malformed {
                            kind: "normalized-test-run".into(),
                            message: err.to_string(),
                        })
                        .map(|bytes| (bytes, failed_cases))
                })
                .map(|(bytes, failed_cases)| ("normalized-test-run", bytes, failed_cases)),
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
                .map(|bytes| ("coverage", bytes, 0)),
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
                .map(|bytes| ("coverage", bytes, 0)),
            _ => continue,
        };
        match normalized {
            Ok((normalized_kind, bytes, failed_cases)) => {
                if failed_cases > 0 {
                    set_record_error(
                        record,
                        format!(
                            "runner artifact {display_path} reports {failed_cases} failed or errored test case(s)"
                        ),
                    );
                }
                record.artifacts.push(ProducedArtifact {
                    kind: normalized_kind.into(),
                    path: format!("{display_path}#normalized"),
                    bytes,
                });
            }
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
    path.strip_prefix(repo).ok().map_or_else(
        || path.display().to_string(),
        |relative| {
            if relative.as_os_str().is_empty() {
                ".".into()
            } else {
                relative.display().to_string()
            }
        },
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

    #[test]
    fn live_service_canonicalizes_existing_repository_paths() {
        let root = TempDir::new("canonical-repo");
        let dotted = root.0.join(".");
        let service = LiveService::new(&dotted);
        assert_eq!(service.repo, canonical_repo_path(&dotted));
        assert!(service.repo.is_absolute());
        #[cfg(windows)]
        assert!(!service.repo.to_string_lossy().starts_with(r"\\?\"));
    }

    #[test]
    fn graph_symbol_ids_resolve_to_repository_test_paths() {
        assert_eq!(
            test_path_from_node_id("symbol:src/widget/Widget.test.tsx#renders"),
            Some("src/widget/Widget.test.tsx".into())
        );
        assert_eq!(
            test_path_from_node_id("file:src/widget/Widget.test.tsx"),
            Some("src/widget/Widget.test.tsx".into())
        );
        assert_eq!(
            test_path_from_node_id("file:src/widget/Widget.stories.tsx"),
            Some("src/widget/Widget.stories.tsx".into())
        );
    }

    #[test]
    fn an_impacted_story_routes_only_to_the_storybook_vitest_project() {
        let root = TempDir::new("storybook-impact-routing");
        std::fs::create_dir_all(root.0.join("src/widget")).unwrap();
        let story = "src/widget/Widget.stories.tsx";
        std::fs::write(root.0.join(story), "export const Default = {};").unwrap();
        let impact = wvq_intelligence::ImpactedSurface {
            head_only: vec![format!("file:{story}")],
            ..wvq_intelligence::ImpactedSurface::default()
        };
        let diff = json!({
            "counts": {
                "nodes_added": 0,
                "nodes_removed": 0,
                "nodes_changed": 0,
                "edges_added": 0,
                "edges_removed": 0
            },
            "nodes": {"added": [], "removed": [], "changed": []},
            "edges": {"added": [], "removed": []}
        });
        let selection = build_live_selection(
            &root.0,
            &json!({"tests": []}),
            &diff,
            &impact,
            &[],
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(selection.selected, [story]);
        assert!(selection.explanations[0][0].contains("base/head Weavatrix impact union"));

        let targets = vec![
            ExecutorTarget {
                executor: wvq_runtime::ExecutorId::new("storybook-vitest-v8").unwrap(),
                cwd: root.0.clone(),
            },
            ExecutorTarget {
                executor: wvq_runtime::ExecutorId::new("vitest").unwrap(),
                cwd: root.0.clone(),
            },
        ];
        let (requests, scope, _, executed) =
            build_execution_requests(&root.0, &targets, &selection, &BTreeSet::new(), "impacted");
        assert_eq!(scope, "impacted");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].target.executor.as_str(), "storybook-vitest-v8");
        assert_eq!(requests[0].filters, [story]);
        assert_eq!(executed, Some(BTreeSet::from([story.into()])));
    }

    #[test]
    fn normalized_suite_identity_resolves_from_a_nested_runner_root() {
        let root = TempDir::new("nested-suite-identity");
        std::fs::create_dir_all(root.0.join("frontend/tests")).unwrap();
        std::fs::write(root.0.join("frontend/tests/cart.test.ts"), "test").unwrap();
        let record = ExecutorRecord {
            executor: "vitest".into(),
            cwd: "frontend".into(),
            selection: Vec::new(),
            status_code: Some(0),
            passed: true,
            error: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            artifacts: Vec::new(),
        };
        let binding = TestBinding {
            path: "frontend/tests/cart.test.ts".into(),
            runner: Some("vitest".into()),
            suite: None,
            case: Some("viewer cannot delete".into()),
            obligations: BTreeSet::from(["viewer-deny".into()]),
            cost: 10,
            flake_penalty: 0,
        };

        assert!(normalized_suite_matches(
            &root.0,
            &record,
            &binding,
            "tests/cart.test.ts"
        ));
        assert!(!normalized_suite_matches(
            &root.0,
            &record,
            &binding,
            "tests/other.test.ts"
        ));
    }

    #[test]
    fn large_file_selection_batches_into_one_bounded_runner_process() {
        let root = TempDir::new("filter-amplification");
        let selected = (0..17)
            .map(|index| {
                let path = format!("tests/case-{index}.test.ts");
                std::fs::create_dir_all(root.0.join("tests")).unwrap();
                std::fs::write(root.0.join(&path), "test").unwrap();
                path
            })
            .collect::<Vec<_>>();
        let selection = LiveSelection {
            selected,
            explanations: Vec::new(),
            uncovered_mandatory: Vec::new(),
            uncovered_all: Vec::new(),
            bindings: Vec::new(),
        };
        let targets = vec![ExecutorTarget {
            executor: wvq_runtime::ExecutorId::new("vitest").unwrap(),
            cwd: root.0.clone(),
        }];
        let (requests, scope, reason, executed) =
            build_execution_requests(&root.0, &targets, &selection, &BTreeSet::new(), "impacted");
        assert_eq!(scope, "impacted");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].filters.len(), 17);
        assert_eq!(requests[0].selected_tests.len(), 17);
        assert_eq!(executed.as_ref().map(BTreeSet::len), Some(17));
        assert!(reason.contains("17 test paths"), "{reason}");
        assert!(reason.contains("1 bounded runner process"), "{reason}");
    }

    #[test]
    fn generic_npm_script_widens_instead_of_assuming_path_filter_support() {
        let root = TempDir::new("npm-filter-safety");
        std::fs::create_dir_all(root.0.join("tests")).unwrap();
        std::fs::write(root.0.join("tests/case.test.ts"), "test").unwrap();
        let selection = LiveSelection {
            selected: vec!["tests/case.test.ts".into()],
            explanations: Vec::new(),
            uncovered_mandatory: Vec::new(),
            uncovered_all: Vec::new(),
            bindings: Vec::new(),
        };
        let targets = vec![ExecutorTarget {
            executor: wvq_runtime::ExecutorId::new("npm-test").unwrap(),
            cwd: root.0.clone(),
        }];

        let (requests, scope, reason, executed) =
            build_execution_requests(&root.0, &targets, &selection, &BTreeSet::new(), "impacted");
        assert_eq!(scope, "all");
        assert_eq!(requests.len(), 1);
        assert!(requests[0].filters.is_empty());
        assert!(executed.is_none());
        assert!(
            reason.contains("no filterable registered executor"),
            "{reason}"
        );
    }

    fn record(executor: &str) -> ExecutorRecord {
        ExecutorRecord {
            executor: executor.into(),
            cwd: ".".into(),
            selection: Vec::new(),
            status_code: Some(0),
            passed: true,
            error: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            artifacts: Vec::new(),
        }
    }

    #[test]
    fn live_test_analytics_persist_failures_durations_and_flaky_history() {
        let root = TempDir::new("live-test-analytics");
        let store = Store::open(&root.0).unwrap();
        let revision = RevisionId::new("revision-test-analytics").unwrap();

        let failed_run = RunId::new("run-test-analytics-failed").unwrap();
        store
            .put_run(&StoredRun {
                id: failed_run.clone(),
                change_id: "test-analytics".into(),
                revision: revision.clone(),
                status: "complete".into(),
                passed: false,
                outcome: "failed".into(),
            })
            .unwrap();
        let mut failed_record = record("vitest");
        failed_record.passed = false;
        failed_record.artifacts.push(ProducedArtifact {
            kind: "normalized-test-run".into(),
            path: "junit.xml#normalized".into(),
            bytes: serde_json::to_vec(&NormalizedTestRun {
                cases: vec![wvq_runtime::TestCaseResult {
                    name: "loads the cart".into(),
                    suite: "src/cart.test.ts".into(),
                    status: TestStatus::Fail,
                    duration_ms: Some(8_000),
                    message: Some("timed out waiting for response".into()),
                }],
                coverage: None,
                raw_artifacts: Vec::new(),
            })
            .unwrap(),
        });
        let failed =
            persist_test_analytics(&store, &failed_run, &revision, &[failed_record], &[]).unwrap();
        assert_eq!(failed.recorded_test_count, 1);
        assert_eq!(failed.failed_test_count, 1);
        assert_eq!(failed.flaky_test_count, 0);
        assert_eq!(failed.unknown_failure_count, 0);

        let passed_run = RunId::new("run-test-analytics-passed").unwrap();
        store
            .put_run(&StoredRun {
                id: passed_run.clone(),
                change_id: "test-analytics".into(),
                revision: revision.clone(),
                status: "complete".into(),
                passed: true,
                outcome: "passed".into(),
            })
            .unwrap();
        let mut passed_record = record("vitest");
        passed_record.artifacts.push(ProducedArtifact {
            kind: "normalized-test-run".into(),
            path: "junit.xml#normalized".into(),
            bytes: serde_json::to_vec(&NormalizedTestRun {
                cases: vec![wvq_runtime::TestCaseResult {
                    name: "loads the cart".into(),
                    suite: "src/cart.test.ts".into(),
                    status: TestStatus::Pass,
                    duration_ms: Some(2_000),
                    message: None,
                }],
                coverage: None,
                raw_artifacts: Vec::new(),
            })
            .unwrap(),
        });
        let passed =
            persist_test_analytics(&store, &passed_run, &revision, &[passed_record], &[]).unwrap();
        assert_eq!(passed.recorded_test_count, 1);
        assert_eq!(passed.failed_test_count, 0);
        assert_eq!(passed.flaky_test_count, 1);
        assert_eq!(passed.unknown_failure_count, 0);

        let value: Value = serde_json::from_slice(&passed.bytes).unwrap();
        assert_eq!(value["slowest_tests"][0]["historical_average_ms"], 5_000);
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
    fn generated_runner_report_is_cleared_without_touching_user_report_paths() {
        let root = TempDir::new("clear-runner-artifacts");
        std::fs::create_dir_all(root.0.join(".weavatrix-quality")).unwrap();
        let generated = root.0.join(".weavatrix-quality/junit.xml");
        let user_owned = root.0.join("junit.xml");
        std::fs::write(&generated, "stale generated evidence").unwrap();
        std::fs::write(&user_owned, "repository-owned evidence").unwrap();

        clear_generated_runner_artifacts(&root.0).unwrap();

        assert!(!generated.exists());
        assert_eq!(
            std::fs::read_to_string(user_owned).unwrap(),
            "repository-owned evidence"
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
    fn failed_junit_fails_the_record_even_when_the_process_exits_zero() {
        let root = TempDir::new("failed-junit-zero-exit");
        let started = SystemTime::now();
        std::fs::write(
            root.0.join("junit.xml"),
            "<testsuite name=\"suite\" failures=\"1\"><testcase name=\"fails\"><failure message=\"boom\"/></testcase></testsuite>",
        )
        .unwrap();
        let mut record = record("storybook-vitest-v8");

        attach_normalized_artifacts(&root.0, &root.0, started, &mut record);

        assert!(!record.passed);
        assert!(
            record.error.as_deref().is_some_and(|message| {
                message.contains("reports 1 failed or errored test case")
            })
        );
        assert!(
            record
                .artifacts
                .iter()
                .any(|artifact| artifact.kind == "normalized-test-run")
        );
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
        let graph = json!({"nodes": [diff["nodes"]["changed"][0]["after"].clone()]});
        let protection =
            live_protection_snapshot(Path::new("."), &revision, &graph, &[record], &[])
                .unwrap()
                .unwrap();
        let flow = protection.flow("symbol:add").unwrap();
        assert_eq!(flow.revision, "revision-1");
        assert_eq!(flow.covered_nodes, ["symbol:add"]);
        assert_eq!(flow.tests, ["executor:cargo-test@."]);
    }

    #[test]
    fn a_single_normalized_case_owns_its_coverage_and_obligation() {
        let root = TempDir::new("exact-protection-case");
        let coverage = CoverageArtifact {
            files: vec![wvq_runtime::FileCoverage {
                path: "service/permission.go".into(),
                covered: vec![wvq_runtime::LineRange { start: 3, end: 5 }],
                uncovered: Vec::new(),
            }],
        };
        let mut record = record("go-test");
        record.cwd = "service".into();
        record.artifacts.push(ProducedArtifact {
            kind: "normalized-test-run".into(),
            path: "go-test#normalized".into(),
            bytes: serde_json::to_vec(&NormalizedTestRun {
                cases: vec![wvq_runtime::TestCaseResult {
                    name: "TestViewerCannotDelete".into(),
                    suite: "fixture.local/product/service".into(),
                    status: TestStatus::Pass,
                    duration_ms: Some(1),
                    message: None,
                }],
                coverage: None,
                raw_artifacts: Vec::new(),
            })
            .unwrap(),
        });
        record.artifacts.push(ProducedArtifact {
            kind: "coverage".into(),
            path: "go-cover.out#normalized".into(),
            bytes: serde_json::to_vec(&coverage).unwrap(),
        });
        let binding = TestBinding {
            path: "service/permission_test.go".into(),
            runner: Some("go-test".into()),
            suite: Some("fixture.local/product/service".into()),
            case: Some("TestViewerCannotDelete".into()),
            obligations: BTreeSet::from(["viewer-deny".into()]),
            cost: 10,
            flake_penalty: 0,
        };
        let revision = RevisionId::new("revision-exact-protector").unwrap();
        let graph = json!({"nodes": [{
            "id": "function:service/permission.go:CanDelete",
            "span": {"file": "service/permission.go", "start_line": 3, "end_line": 5}
        }]});

        let protection =
            live_protection_snapshot(&root.0, &revision, &graph, &[record], &[binding])
                .unwrap()
                .unwrap();
        let flow = protection
            .flow("function:service/permission.go:CanDelete")
            .unwrap();
        assert_eq!(
            flow.tests,
            ["service/permission_test.go#TestViewerCannotDelete"]
        );
        assert_eq!(
            protection.executed_tests,
            ["service/permission_test.go#TestViewerCannotDelete"]
        );
        assert_eq!(flow.proven_obligations, ["viewer-deny"]);
    }

    #[test]
    fn a_passing_case_with_no_remaining_impacted_flow_is_phantom_not_removed() {
        let exact = "service/permission_test.go#TestViewerCannotDelete";
        let base = snapshot_with_executed_tests(
            &RevisionId::new("base-protector-inventory").unwrap(),
            vec![FlowProtection {
                flow: "symbol:service/permission.go#CanDelete".into(),
                revision: "base-protector-inventory".into(),
                tests: vec![exact.into()],
                sessions: Vec::new(),
                covered_nodes: vec!["symbol:service/permission.go#CanDelete".into()],
                covered_branches: Vec::new(),
                proven_obligations: vec!["viewer-deny".into()],
                proofs: Vec::new(),
            }],
            vec![exact.into()],
        )
        .unwrap();
        let head = snapshot_with_executed_tests(
            &RevisionId::new("head-protector-inventory").unwrap(),
            vec![FlowProtection {
                flow: "symbol:service/permission.go#CanDelete".into(),
                revision: "head-protector-inventory".into(),
                tests: Vec::new(),
                sessions: Vec::new(),
                covered_nodes: Vec::new(),
                covered_branches: Vec::new(),
                proven_obligations: Vec::new(),
                proofs: Vec::new(),
            }],
            vec![exact.into()],
        )
        .unwrap();

        let lineage = protection_lineage(&base, &head);

        assert_eq!(lineage.len(), 1);
        assert_eq!(lineage[0].test, exact);
        assert_eq!(lineage[0].state, "unchanged");
        assert!(lineage[0].phantom);
        assert_eq!(
            lineage[0].lost_flows,
            ["symbol:service/permission.go#CanDelete"]
        );
    }

    #[test]
    fn batch_coverage_never_claims_which_normalized_case_protected_a_flow() {
        let mut record = record("go-test");
        record.cwd = "service".into();
        record.artifacts.push(ProducedArtifact {
            kind: "normalized-test-run".into(),
            path: "go-test#normalized".into(),
            bytes: serde_json::to_vec(&NormalizedTestRun {
                cases: ["TestViewerCannotDelete", "TestAdminCanDelete"]
                    .into_iter()
                    .map(|name| wvq_runtime::TestCaseResult {
                        name: name.into(),
                        suite: "fixture.local/product/service".into(),
                        status: TestStatus::Pass,
                        duration_ms: Some(1),
                        message: None,
                    })
                    .collect(),
                coverage: None,
                raw_artifacts: Vec::new(),
            })
            .unwrap(),
        });

        let inventory =
            executed_test_inventory(Path::new("."), std::slice::from_ref(&record), &[]).unwrap();
        assert_eq!(
            inventory,
            [
                "go-test:fixture.local/product/service#TestAdminCanDelete",
                "go-test:fixture.local/product/service#TestViewerCannotDelete"
            ],
            "exact case execution is retained even though batch coverage stays executor-level"
        );

        let protectors = coverage_protectors(Path::new("."), &record, &[]).unwrap();
        assert_eq!(
            protectors,
            [CoverageProtector {
                identity: "executor:go-test@service".into(),
                obligations: Vec::new(),
            }]
        );
    }

    #[test]
    fn dynamic_selection_learns_only_repeated_single_test_coverage() {
        let root = TempDir::new("dynamic-selection-history");
        let store = Store::open(&root.0).unwrap();
        let revision = RevisionId::new("revision-dynamic-selection").unwrap();
        let graph = json!({
            "nodes": [{
                "id": "symbol:add",
                "span": {"file": "src/lib.rs", "start_line": 1, "end_line": 2}
            }]
        });
        let coverage = CoverageArtifact {
            files: vec![wvq_runtime::FileCoverage {
                path: "src/lib.rs".into(),
                covered: vec![wvq_runtime::LineRange { start: 1, end: 2 }],
                uncovered: Vec::new(),
            }],
        };

        for index in 1..=2 {
            let run_id = RunId::new(format!("run-dynamic-selection-{index}")).unwrap();
            store
                .put_run(&StoredRun {
                    id: run_id.clone(),
                    change_id: "dynamic-selection".into(),
                    revision: revision.clone(),
                    status: "complete".into(),
                    passed: true,
                    outcome: "passed".into(),
                })
                .unwrap();
            let mut exact = record("vitest");
            exact.selection = vec!["tests/add.test.ts".into()];
            exact.artifacts.push(ProducedArtifact {
                kind: "coverage".into(),
                path: "coverage#normalized".into(),
                bytes: serde_json::to_vec(&coverage).unwrap(),
            });
            persist_dynamic_coverage_history(&store, &run_id, &revision, &graph, &[exact]).unwrap();
        }

        let learned = store
            .historical_tests_for_nodes(&["symbol:add".into()], 2, 100)
            .unwrap();
        assert_eq!(learned.len(), 1);
        assert_eq!(learned[0].test_path, "tests/add.test.ts");

        let batch_run = RunId::new("run-dynamic-selection-batch").unwrap();
        store
            .put_run(&StoredRun {
                id: batch_run.clone(),
                change_id: "dynamic-selection".into(),
                revision: revision.clone(),
                status: "complete".into(),
                passed: true,
                outcome: "passed".into(),
            })
            .unwrap();
        let mut batch = record("vitest");
        batch.selection = vec!["tests/a.test.ts".into(), "tests/b.test.ts".into()];
        batch.artifacts.push(ProducedArtifact {
            kind: "coverage".into(),
            path: "coverage#normalized".into(),
            bytes: serde_json::to_vec(&coverage).unwrap(),
        });
        persist_dynamic_coverage_history(&store, &batch_run, &revision, &graph, &[batch]).unwrap();
        assert_eq!(
            store
                .historical_tests_for_nodes(&["symbol:add".into()], 1, 100)
                .unwrap()
                .len(),
            1,
            "aggregate coverage from a multi-test batch is not attributed"
        );
    }

    #[test]
    fn repeated_dynamic_history_is_unioned_with_weavatrix_selection() {
        let root = TempDir::new("dynamic-selection-union");
        std::fs::create_dir_all(root.0.join("tests")).unwrap();
        std::fs::write(root.0.join("tests/history.test.ts"), "test").unwrap();
        let static_report = json!({"tests": []});
        let diff = json!({
            "counts": {
                "nodes_added": 0,
                "nodes_removed": 0,
                "nodes_changed": 0,
                "edges_added": 0,
                "edges_removed": 0
            },
            "nodes": {"added": [], "removed": [], "changed": []},
            "edges": {"added": [], "removed": []}
        });
        let selection = build_live_selection(
            &root.0,
            &static_report,
            &diff,
            &wvq_intelligence::ImpactedSurface::default(),
            &[],
            &[],
            &[HistoricalTestCandidate {
                test_path: "tests/history.test.ts".into(),
                matched_nodes: vec!["symbol:history".into()],
                minimum_observations: 2,
                defensive_misses: 0,
                last_revision: RevisionId::new("revision-history").unwrap(),
            }],
        )
        .unwrap();
        assert_eq!(selection.selected, ["tests/history.test.ts"]);
        assert!(selection.explanations[0][0].contains("repeated measured coverage"));
    }

    #[test]
    fn defensive_full_run_miss_is_persisted_and_teaches_future_selection() {
        let root = TempDir::new("defensive-selection-audit");
        std::fs::create_dir_all(root.0.join("tests")).unwrap();
        std::fs::write(root.0.join("tests/missed.test.ts"), "test").unwrap();
        let store = Store::open(&root.0).unwrap();
        let revision = RevisionId::new("revision-selection-audit").unwrap();
        let impacted = RunId::new("run-selection-audit-impacted").unwrap();
        let full = RunId::new("run-selection-audit-full").unwrap();
        for (run, passed, outcome) in [(&impacted, true, "passed"), (&full, false, "failed")] {
            store
                .put_run(&StoredRun {
                    id: run.clone(),
                    change_id: "selection-audit".into(),
                    revision: revision.clone(),
                    status: "complete".into(),
                    passed,
                    outcome: outcome.into(),
                })
                .unwrap();
        }
        let mut handles = Vec::new();
        put_json_run_artifact(
            &store,
            &impacted,
            "artifact-selection-audit-impacted-summary",
            "execution-summary",
            &json!({"requested_scope": "impacted", "effective_scope": "impacted"}),
            &mut handles,
        )
        .unwrap();
        put_json_run_artifact(
            &store,
            &impacted,
            "artifact-selection-audit-impact",
            "impacted-surface",
            &json!({
                "base_only": [],
                "head_only": ["symbol:cart"],
                "shared": [],
                "removed_nodes": [],
                "removed_edges": [],
                "removed_surfaces": []
            }),
            &mut handles,
        )
        .unwrap();
        put_json_run_artifact(
            &store,
            &full,
            "artifact-selection-audit-full-summary",
            "execution-summary",
            &json!({"requested_scope": "all", "effective_scope": "all"}),
            &mut handles,
        )
        .unwrap();
        store
            .put_test_case_result(&StoredTestCaseResult {
                id: "selection-audit-missed-case".into(),
                run_id: full.clone(),
                revision: revision.clone(),
                executor: "vitest".into(),
                suite: "tests/missed.test.ts".into(),
                name: "detects the regression".into(),
                status: "fail".into(),
                duration_ms: Some(10),
                fingerprint: None,
            })
            .unwrap();

        let audit =
            audit_live_selection(&root.0, &store, impacted.as_str(), full.as_str()).unwrap();
        assert_eq!(audit.status, "contradicted");
        assert_eq!(audit.missed_failure_count, 1);
        assert_eq!(audit.learned_test_count, 1);
        assert!(audit.evidence_handle.is_some());
        assert_eq!(
            audit_live_selection(&root.0, &store, impacted.as_str(), full.as_str(),).unwrap(),
            audit,
            "replaying the same audit is idempotent"
        );
        let learned = store
            .historical_tests_for_nodes(&["symbol:cart".into()], 2, 100)
            .unwrap();
        assert_eq!(learned.len(), 1);
        assert_eq!(learned[0].test_path, "tests/missed.test.ts");
        assert_eq!(learned[0].defensive_misses, 1);
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

    #[test]
    fn responsive_probe_retries_only_incomplete_evidence() {
        let complete = ResponsiveProbe {
            width: 767,
            delta: UiIntegrityDelta::default(),
        };
        assert!(!responsive_probe_incomplete(&complete));

        let mut truncated = complete.clone();
        truncated.delta.truncated = true;
        assert!(responsive_probe_incomplete(&truncated));

        let mut unmeasured = complete;
        unmeasured
            .delta
            .unmeasured_states
            .push("checkout#0@/@767x720".into());
        assert!(responsive_probe_incomplete(&unmeasured));
    }

    #[test]
    fn run_policy_caps_program_owned_browser_capture() {
        let raw = r#"{
            "schema_v": 1,
            "id": "capture-policy",
            "source": "authored",
            "obligations": ["visible"],
            "steps": [{"action":"assert","obligation":"visible"}],
            "evidence_policy": {
                "screenshot":"always",
                "trace":"always",
                "network":"always",
                "console":"always",
                "storage":"always"
            }
        }"#;
        let mut minimal = TestProgram::from_json(raw).unwrap();
        cap_browser_evidence(&mut minimal, "minimal");
        assert_eq!(minimal.evidence_policy.screenshot, CaptureWhen::OnFailure);
        assert_eq!(minimal.evidence_policy.trace, CaptureWhen::OnFailure);
        assert!(!browser_capture_active(
            CaptureWhen::Always,
            false,
            "minimal"
        ));
        assert!(browser_capture_active(CaptureWhen::Always, true, "minimal"));

        let mut none = TestProgram::from_json(raw).unwrap();
        cap_browser_evidence(&mut none, "none");
        assert_eq!(none.evidence_policy.screenshot, CaptureWhen::Never);
        assert_eq!(none.evidence_policy.network, CaptureWhen::Never);
        assert!(!browser_capture_active(CaptureWhen::Always, true, "none"));
    }
}
