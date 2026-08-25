//! Domain facade. CLI and MCP call this; they do not reimplement policy.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
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
    AiAxis, AiCallKind, AiCostFirewall, AiUsage, AssemblyInput, AxisState,
    ChangeQualityVerdict, DebtAxis, DebtItem, DeltaContext, DeltaFindingRef,
    DeltaTriangleAxis, ExecutionEvidence, FailureEvidence, FlakeClass, FlowProtection, FlowView,
    HealEdit, Limitation, LocalModelRequest, OracleReplacementReview,
    ProofOutcome, ProofVerdict, ProtectionAxis, ProtectionCheckInput, ProtectionDelta,
    ProtectionDeltaState, ProtectionFinding, ProtectionPolicy, ProtectionSnapshot, ProtectionView,
    StabilityAxis, TestChange, TestLineageView, TimingBucket, UiFindingRef, UiIntegrityAxis,
    VerdictInputs, apply_heal, assemble, call_local_model, compose,
    debt_rule_blocks, fingerprint_id, gate_protection, protection_delta,
    snapshot_with_executed_tests, summarise, triage,
};
use wvq_runtime::{
    BehaviorState, BrowserAssertionStatus, BrowserProgramRun,
    BrowserRecordingRequest, BrowserRunConfig, BrowserViewport, CaptureWhen, CoverageArtifact,
    ExecutionResult, ExecutorRegistry, ExecutorTarget, NetworkMode,
    NetworkRunPolicy, NormalizedTestRun, PrepareRequest, ProgramOracle, Recorder,
    TestAction, TestProgram, TestStatus,
    default_limits, discover_executor_targets, parse_cargo_test, parse_go_coverprofile,
    parse_go_json, parse_junit, parse_lcov, promote, record_browser_session, run_browser_program,
    run_browser_program_at,
};
use wvq_spec::{
    EvidenceKind, RiskLevel, SpecError, TestObligation, load_quality_contract, seal,
};
use wvq_spec_recovery::{
    NarrativeInput, RecoveryDesk, RecoveryInput,
    TestIntentSummary, VerifyContext, cluster, narrate,
};
use wvq_store::{
    HistoricalTestCandidate, Store, StoredAiUsage, StoredProof, StoredRun,
    StoredRunItem, StoredSelectionAudit, StoredTestCaseIdentity, StoredTestCaseResult,
};
use wvq_ui::{
    LayoutSnapshot, ResponsiveProbe, UiFindingState, UiIntegrityDelta, UiIntegrityFinding,
    UiIntegrityPolicy, UiIntegritySnapshot, detect as detect_ui, next_responsive_probe,
    ratchet as ratchet_ui, responsive_failure_intervals,
    responsive_probe_plan,
};

use crate::commands::{
    AuthorDraftCommand, AuthorHealCommand, AuthorHealEdit, AuthorPreviewCommand,
    AuthorPromoteCommand, AuthorValidateCommand, ChangesCommand, Command, ContextCommand,
    DebtCommand, EvidenceCommand, ExplainCommand, ModelCommand, PlanCommand, RecordCommand,
    RecoveryCommand, RunCommand, SelectCommand, SpecCommand, StatusCommand, VerifyCommand,
};
use crate::replies::{
    AuthorDraftReply, AuthorHealReply, AuthorModelUsage, AuthorPreviewReply, AuthorPromoteReply,
    AuthorValidateReply, AuthoringObligation, ChangesReply, ContextReply, DebtReply, EvidenceReply,
    ExplainReply, INLINE_LIMIT, ModelReply, PlanReply, ProofSummary, RecordReply, RecoveryReply,
    Reply, RunReply, SelectReply, SelectionAuditReply, SpecSealReply, SpecValidateReply,
    StatusReply, VerifyReply, bound_items,
};
use crate::source_mutation::{
    MutationBinding, MutationPolicy, MutationRunDocument, MutationRunRequest,
    execute_source_mutation,
};

/// CAS artifact kind holding the base/head UI-integrity ratchet for one run.
pub(in crate::service) const UI_INTEGRITY_DELTA_KIND: &str = "ui-integrity-delta";
pub(in crate::service) const MUTATION_RESULTS_KIND: &str = "mutation-results";

mod authoring;
mod recovery;
mod policy;
mod delta;
mod types;
mod access;
mod validate;
mod git;
mod graph;
mod paths;
mod verify_reply;
mod verify_axes;
mod verify_json;
mod verify_debt;
mod selection_build;
mod selection_audit;
mod execute;
mod persist_run;
mod persist_browser;
mod persist_ui;
mod persist_ui_analyse;
mod persist_behavior;
mod persist_evidence;
mod impact;
mod protection_snapshot;
mod protection_coverage;
mod protection_view;
mod protection_lineage;
mod protection_graph_extra;
mod analytics;
mod runner;
mod runner_coverage;

use authoring::{
    author_preview_token, authoring_authority_tokens, authoring_context, authoring_model_prompt,
    authoring_obligations, deterministic_checks, empty_debt,
    map_authoring_store_error, obligation_kind_token, obligation_texts, pack_context,
    persist_author_preview, requirement_texts, risk_token, unique_requirements,
    validate_author_candidate, validate_authoring_budget, working_tree_selection,
};
use policy::{
    browser_test_bindings, load_browser_policy, load_browser_policy_with,
    load_browser_runtime_with, load_debt_exceptions, load_live_browser_policy, load_model_policy,
    load_test_bindings, load_ui_integrity_policy, ui_collection_config,
};
use recovery::{
    recovery_candidates, recovery_code_delta, recovery_commits, recovery_evidence,
    recovery_existing_requirements,
};

use delta::{declared_code_flows, persist_delta_triangle};
use types::*;
use validate::*;
use git::*;
use graph::*;
use paths::*;
use verify_reply::{
    verify_from_token,
    parse_proof_verdict,
    explain_ui_finding,
    artifact_handle_of_kind,
    explain_stored_proof,
    stored_range,
    snapshot_artifact,
    stored_oracle_replacement,
};
use verify_axes::{
    protection_axis_from,
    debt_axis_from,
    stability_axis,
    ui_integrity_axis,
    delta_triangle_axis,
};
use verify_json::{
    parse_axis_state,
    parse_ui_findings,
    json_string,
    json_u64,
    mandatory_test_paths,
};
use verify_debt::{
    combine_verify,
    combine_verdicts,
    count_field,
    debt_bucket_ids,
    compact_debt_findings,
    explain_debt_finding,
};
use selection_build::{
    static_and_base_tests,
    historical_selection_candidates,
    merge_historical_selection,
    merge_impacted_stories,
    build_live_selection,
    merged_test_bindings,
    selection_candidates,
    live_selection_report,
    SelectionAuditArtifactInput,
};
use selection_audit::{
    audit_live_selection,
    load_shadow_runs,
    persist_selection_audit_artifact,
    stored_selection_audit_reply,
    validate_shadow_scopes,
    missed_failure_identities,
    impact_nodes_from_artifact,
    resolve_observed_test_path,
    read_single_run_json,
};
use execute::{
    build_execution_requests,
    batch_filter_groups,
    supports_path_filters,
    target_accepts_filter,
    available_test_paths,
    full_execution_requests,
    execute_full_targets,
};
use persist_run::{
    make_run_id,
    make_ai_usage_id,
    put_run_artifact,
    put_json_run_artifact,
    obligation_execution_map,
    normalized_suite_matches,
    normalized_status,
    severity_token,
};
use persist_browser::{
    persist_browser_runs,
    persist_browser_run,
    persist_browser_observations,
    persist_browser_files,
    stored_browser_assertions,
    BEHAVIOR_SAMPLE_LIMIT,
    BEHAVIOR_PROGRAM_SAMPLE_LIMIT,
    MAX_UI_ARTIFACT_BYTES,
    MAX_UI_REPLY_FINDINGS,
};
use persist_ui::{
    ui_delta_document,
    responsive_probe_incomplete,
    ui_finding_refs_with_intervals,
    ui_finding_refs,
    persist_ui_integrity,
};
use persist_ui_analyse::{
    CollectedUi,
    analyse_ui_snapshots,
    duplicate_mutation_finding,
    hit_test_summary,
    put_bounded_ui_artifact,
};
use persist_behavior::{
    persist_browser_behavior,
    persist_program_behavior,
    normalized_behavior_state,
    persist_behavior_edge,
    program_behavior_artifact,
    bounded_set,
    bounded_network_operation,
    recorded_api_operation,
};
use persist_evidence::{
    remove_browser_evidence_file,
    capture_active,
    browser_capture_active,
    browser_evidence_kinds,
    cap_browser_evidence,
    parse_obligation_execution_map,
    parse_revision_range_evidence,
    valid_commit_id,
};
use impact::{
    merge_browser_proof_evidence,
    live_impacted_surface,
};
use protection_snapshot::{
    ensure_complete_diff,
    live_protection_snapshot,
    executed_test_inventory,
    persist_dynamic_coverage_history,
};
use protection_coverage::{
    measured_protection_flows,
    CoverageProtector,
    coverage_protectors,
    coverage_graph_mismatch,
};
use protection_view::{
    expectation_change,
    build_protection_view,
};
use protection_lineage::{
    protection_test_changes,
    approved_replaced_flows,
    replacement_test_for_flow,
    test_identity_has_path,
    graph_relocations,
    snapshot_relocations,
    stable_symbol_signature,
    protection_lineage,
};
use protection_graph_extra::{
    graph_singleton_path,
    PersistedTestAnalytics,
    TestAnalyticsDocument,
    TestOutcomeCounts,
    ObservedTestCase,
};
use analytics::{
    collect_observed_test_cases,
    persist_failure,
    persist_test_analytics,
    test_status_token,
    failure_timing_bucket,
    flake_class_token,
};
use runner::{
    execution_summary,
    MAX_RUNNER_ARTIFACT_BYTES,
    clear_generated_runner_artifacts,
    attach_normalized_artifacts,
};
use runner_coverage::{
    ARTIFACT_CLOCK_TOLERANCE,
    normalize_coverage_paths,
    read_go_module,
    runner_artifact_candidates,
    artifact_is_fresh,
    set_record_error,
    stdout_kind,
};

/// CAS artifact kind holding live same-program Spec x Code x Behavior evidence.
pub(in crate::service) const DELTA_TRIANGLE_KIND: &str = "delta-triangle";

/// CAS artifact kind for the exact expectation replacement a QA reviewed.
pub(in crate::service) const ORACLE_REPLACEMENT_KIND: &str = "oracle-replacement-proposal";

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
    /// Passively capture natural app use, discard redundant traces, and preview a useful replay.
    ///
    /// Promotion remains an explicit, same-seal operation; recording cannot mutate an oracle.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] on invalid revision, browser, seal, or evidence.
    fn record(&self, cmd: &RecordCommand) -> Result<RecordReply, BusError> {
        self.record_controlled(cmd, Arc::new(AtomicBool::new(false)))
    }
    /// Passive recording with transport-owned cooperative cancellation.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`QualityService::record`].
    fn record_controlled(
        &self,
        cmd: &RecordCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RecordReply, BusError> {
        let _ = cancel;
        self.record(cmd)
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
        Command::Record(cmd) => service
            .record(&cmd)
            .map(|reply| Reply::Record(Box::new(reply))),
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

    fn record_controlled(
        &self,
        cmd: &RecordCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RecordReply, BusError> {
        let _ = cancel;
        Ok(RecordReply {
            session_id: "recording-fake".into(),
            change: cmd.change.clone(),
            revision: "fake-revision".into(),
            captured_events: 2,
            useful: true,
            discarded: false,
            discard_reason: None,
            new_behavior_states: 2,
            new_behavior_edges: 1,
            linked_obligations: vec!["others-visible".into()],
            new_obligations: vec!["others-visible".into()],
            api_operations: Vec::new(),
            new_api_operations: Vec::new(),
            limitations: Vec::new(),
            candidate: Some(json!({"id": "recorded-fake"})),
            preview: Some(AuthorPreviewReply {
                preview_id: "preview-recorded-fake".into(),
                change: cmd.change.clone(),
                revision: "fake-revision".into(),
                program_id: "recorded-fake".into(),
                passed: true,
                asserted: vec!["others-visible".into()],
                contradicted: Vec::new(),
                failure: None,
                observation_handles: Vec::new(),
                screenshot_handles: Vec::new(),
                trace_handle: None,
                program_persisted: false,
            }),
            trace_handle: Some("artifact-session-recording-fake-trace".into()),
            network_profile_handle: Some("artifact-session-recording-fake-network".into()),
            runtime_llm_tokens: 0,
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
        change: &str,
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
        let spec = optional_change(&worktree.path, change)?;
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
                    network: head_policy.network.clone(),
                    cancel: Arc::clone(&cancel),
                },
                &executable,
                &configured.oracles,
                revision.as_str(),
            )
            .map_err(|err| BusError::Runtime(err.to_string()))?;
            runs.push(result);
        }
        Ok(BaseBrowserReplay {
            revision,
            spec,
            runs,
        })
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
        let head_runtime =
            load_browser_policy(&self.repo, &compiled.obligations)?.ok_or_else(|| {
                BusError::Runtime("no browser runtime is configured for this repository".into())
            })?;
        let engine = head_runtime.module_root;
        let comparison_network = head_runtime.network;
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
                    network: comparison_network.clone(),
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
        let mut base_browser =
            load_browser_policy_with(&base_worktree.path, &compiled.obligations, Some(&engine))?
                .ok_or_else(|| {
                    BusError::Runtime("base has no browser runtime configuration".into())
                })?;
        let head_browser =
            load_browser_policy_with(&self.repo, &compiled.obligations, Some(&engine))?
                .ok_or_else(|| {
                    BusError::Runtime("head has no browser runtime configuration".into())
                })?;
        // Base/head geometry must use the exact same network fixture. Runtime
        // coordinates come from each revision, but the head-selected replay
        // policy is the comparison authority just like the head TestProgram.
        base_browser.network = head_browser.network.clone();
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
                    network: browser.network.clone(),
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
        Self::persist_ui_delta_with_handles(store, run, base, delta, &mut handles)
    }

    fn persist_ui_delta_with_handles(
        store: &Store,
        run: &RunId,
        base: &UiIntegritySnapshot,
        delta: &UiIntegrityDelta,
        handles: &mut Vec<String>,
    ) -> Result<(), BusError> {
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
                handles,
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
            handles,
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
        let mutation_policy =
            MutationPolicy::from_contract(&load_quality_contract(&self.repo, &compiled.change)?)
                .map_err(BusError::Runtime)?;
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
                    exact_case: None,
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

        let mutation_bindings = live_selection
            .bindings
            .iter()
            .filter_map(|binding| {
                Some(MutationBinding {
                    path: binding.path.clone(),
                    runner: binding.runner.clone()?,
                    case: binding.case.clone()?,
                    obligations: binding.obligations.clone(),
                })
            })
            .collect::<Vec<_>>();
        let mutation_document = mutation_policy
            .as_ref()
            .map(|policy| {
                if records.is_empty() || records.iter().any(|record| !record.passed) {
                    Ok(MutationRunDocument::unmeasured(
                        policy,
                        "the selected baseline suite did not pass before mutation".into(),
                    ))
                } else {
                    execute_source_mutation(&MutationRunRequest {
                        repo: &self.repo,
                        head_commit: &range.head_commit,
                        merge_base: &range.merge_base,
                        head_is_worktree: range.head_ref == "WORKTREE",
                        added_files: &changed.added,
                        changed_files: &changed.changed,
                        bindings: &mutation_bindings,
                        policy,
                        executors: &self.executors,
                        cancel: Arc::clone(&cancel),
                    })
                    .map_err(BusError::Runtime)
                }
            })
            .transpose()?;

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
                        network: policy.network.clone(),
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
                    &compiled.change,
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
        if let Some(mutation) = &mutation_document {
            put_json_run_artifact(
                &store,
                &run_id,
                &format!("artifact-{}-mutation-results", run_id.as_str()),
                MUTATION_RESULTS_KIND,
                mutation,
                &mut handles,
            )?;
            for result in &mutation.results {
                let stored_result_id = format!("{}--{}", run_id.as_str(), result.id);
                store
                    .put_mutation_result(
                        &stored_result_id,
                        &result.operator,
                        &format!("{}:{}:{}", result.path, result.line, result.column),
                        &result.status,
                    )
                    .map_err(|err| BusError::Store(err.to_string()))?;
            }
        }
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
        let head_ui = persist_ui_integrity(
            &store,
            &run_id,
            &before,
            &ui_policy,
            &browser_runs,
            &mut handles,
        )?;
        if let (Some(head_ui), Some(base_replay)) = (head_ui.as_ref(), base_browser_replay.as_ref())
        {
            let base_ui = match base_replay {
                Ok(base) if base.runs.len() == browser_runs.len() => {
                    let borrowed = browser_runs
                        .iter()
                        .zip(&base.runs)
                        .map(|((configured, _), result)| (*configured, result.clone()))
                        .collect::<Vec<_>>();
                    analyse_ui_snapshots(&base.revision, &ui_policy, &borrowed)?.snapshot
                }
                Ok(base) => UiIntegritySnapshot {
                    revision: base.revision.to_string(),
                    truncated: true,
                    ..UiIntegritySnapshot::default()
                },
                Err(_) => UiIntegritySnapshot {
                    revision: range.merge_base.clone(),
                    truncated: true,
                    ..UiIntegritySnapshot::default()
                },
            };
            let previously_fixed = store
                .previously_fixed_debt()
                .map_err(|err| BusError::Store(err.to_string()))?
                .into_iter()
                .filter(|item| item.starts_with("ui:"))
                .collect::<BTreeSet<_>>();
            let mut delta = ratchet_ui(&base_ui, head_ui, &previously_fixed, &ui_policy);
            if ui_policy.responsive.enabled && base_replay.is_ok() {
                let (intervals, truncated) = self.measure_responsive_ui(
                    &range,
                    &compiled,
                    &ui_policy,
                    &base_ui,
                    head_ui,
                    &previously_fixed,
                )?;
                delta.responsive_intervals = intervals;
                delta.responsive_truncated = truncated;
            }
            let fixed = delta.fixed_fingerprints();
            if !fixed.is_empty() {
                store
                    .remember_fixed_debt(&fixed, &before)
                    .map_err(|err| BusError::Store(err.to_string()))?;
            }
            Self::persist_ui_delta_with_handles(&store, &run_id, &base_ui, &delta, &mut handles)?;
        }
        persist_dynamic_coverage_history(&store, &run_id, &before, &protection_graph, &records)?;
        let mut code_flows = Vec::new();
        if let Some(protection) = live_protection_snapshot(
            &self.repo,
            &before,
            &protection_graph,
            &records,
            &live_selection.bindings,
        )? {
            code_flows.extend(protection.flows.iter().cloned());
            put_json_run_artifact(
                &store,
                &run_id,
                &format!("artifact-{}-protection", run_id.as_str()),
                "protection-snapshot",
                &protection,
                &mut handles,
            )?;
        }
        let bound_files = live_selection
            .bindings
            .iter()
            .map(|binding| binding.path.clone())
            .collect::<Vec<_>>();
        let bound_graph = protection_graph_for_files(&self.repo, &before, &bound_files)?;
        code_flows.extend(declared_code_flows(
            before.as_str(),
            &live_selection.bindings,
            &bound_graph,
        ));
        if let Some(base_replay) = base_browser_replay {
            persist_delta_triangle(
                &store,
                &run_id,
                &compiled,
                &changed,
                &graph_diff,
                &code_flows,
                &browser_runs,
                base_replay,
                &cmd.evidence_policy,
                &mut handles,
            )?;
        }
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
        let mutation_policy =
            MutationPolicy::from_contract(&contract).map_err(BusError::Runtime)?;
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
        let mut mutation_evidence = None::<MutationRunDocument>;
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
            if record.kind == MUTATION_RESULTS_KIND {
                if mutation_evidence.is_some() {
                    return Err(BusError::Store(
                        "run has more than one mutation-results artifact".into(),
                    ));
                }
                let document: MutationRunDocument =
                    serde_json::from_slice(&bytes).map_err(|err| {
                        BusError::Store(format!("run has malformed mutation-results: {err}"))
                    })?;
                if document.schema_v != 1 {
                    return Err(BusError::Store(format!(
                        "unknown mutation-results schema_v {}",
                        document.schema_v
                    )));
                }
                let policy = mutation_policy.as_ref().ok_or_else(|| {
                    BusError::Store(
                        "run contains mutation evidence not requested by quality.yaml".into(),
                    )
                })?;
                document.validate(policy).map_err(|error| {
                    BusError::Store(format!("run has invalid mutation-results: {error}"))
                })?;
                mutation_evidence = Some(document);
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
                mutation: mutation_policy.as_ref().and_then(|policy| {
                    policy.summary_for(mutation_evidence.as_ref(), obligation.id.as_str())
                }),
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
                network: policy.network,
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

    #[allow(clippy::too_many_lines)]
    fn record_controlled(
        &self,
        cmd: &RecordCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RecordReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let _range = self.revision_range(&cmd.base, &cmd.head)?;
        let before = self.revision()?;
        let policy = load_browser_policy(&self.repo, &compiled.obligations)?.ok_or_else(|| {
            BusError::Runtime(
                "passive recording requires a browser runtime in .weavatrix-quality/config.yaml"
                    .into(),
            )
        })?;
        let mut oracles = Vec::new();
        for obligation in &compiled.obligations {
            let Some(expected) = &obligation.expected else {
                continue;
            };
            oracles.push(ProgramOracle {
                obligation: obligation.id.clone(),
                condition: obligation
                    .condition
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
                expected: serde_json::to_value(expected)
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
            });
        }
        let session_id = author_preview_token("recording")?;
        let idle_timeout = Duration::from_millis(cmd.idle_timeout_ms);
        let bridge_timeout = policy
            .timeout
            .max(idle_timeout.saturating_add(Duration::from_secs(15)))
            .min(Duration::from_secs(120));
        let evidence_dir = self
            .repo
            .join(".weavatrix-quality")
            .join("recording-evidence")
            .join(&session_id);
        let recording = record_browser_session(
            &BrowserRunConfig {
                base_url: policy.base_url,
                browser: policy.browser,
                headless: cmd.headless.unwrap_or(false),
                timeout: bridge_timeout,
                module_root: policy.module_root,
                runtime_dir: self
                    .repo
                    .join(".weavatrix-quality/runtime/playwright-runner"),
                evidence_dir: evidence_dir.clone(),
                viewport: None,
                ui_integrity: None,
                network: NetworkRunPolicy {
                    mode: NetworkMode::Record,
                    profile: None,
                    redact_json_keys: policy.network.redact_json_keys,
                    max_entries: policy.network.max_entries,
                    max_body_bytes: policy.network.max_body_bytes,
                    max_total_bytes: policy.network.max_total_bytes,
                },
                cancel: Arc::clone(&cancel),
            },
            &BrowserRecordingRequest {
                session: session_id.clone(),
                route: cmd.route.clone(),
                fixture_values: cmd.fixture_values.clone(),
                idle_timeout,
                max_events: cmd.max_events,
            },
            &oracles,
        )
        .map_err(|err| BusError::Runtime(err.to_string()))?;
        let after = self.revision()?;
        if before != after {
            return Err(BusError::Ambiguous(format!(
                "repository revision changed during passive recording: `{before}` -> `{after}`"
            )));
        }
        let _ = std::fs::remove_dir(&evidence_dir);

        let initial = BehaviorState::from_observation(&recording.initial).ok_or_else(|| {
            BusError::Runtime("passive recorder initial observation omitted its route".into())
        })?;
        let mut recorder = Recorder::new(&session_id, None, None);
        recorder.start(initial);
        for (name, value) in &cmd.fixture_values {
            recorder.link_fixture(name, Value::String(value.clone()));
        }
        for event in &recording.events {
            let state = BehaviorState::from_observation(&event.observation).ok_or_else(|| {
                BusError::Runtime("passive recorder event omitted its route".into())
            })?;
            recorder
                .step(event.action.clone(), state)
                .map_err(|err| BusError::Runtime(err.to_string()))?;
        }
        for outcome in &recording.obligations {
            if outcome.status == "passed" {
                recorder.link_obligation(
                    wvq_domain::ObligationId::new(&outcome.obligation)
                        .map_err(|err| BusError::Identity(err.to_string()))?,
                );
            }
        }
        let api_operations = recording
            .events
            .iter()
            .flat_map(|event| &event.observation.network_requests)
            .filter(|request| {
                request
                    .resource_type
                    .as_deref()
                    .is_some_and(|kind| matches!(kind, "fetch" | "xhr" | "websocket"))
            })
            .map(recorded_api_operation)
            .collect::<BTreeSet<_>>();
        for operation in &api_operations {
            recorder.link_api(operation);
        }
        let trace = recorder
            .finish()
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        let store = self.store()?;
        let mut new_behavior_states = 0_u64;
        for digest in trace
            .state_digests()
            .map_err(|err| BusError::Runtime(err.to_string()))?
        {
            if !store
                .has_behavior_state(&digest)
                .map_err(|err| BusError::Store(err.to_string()))?
            {
                new_behavior_states = new_behavior_states.saturating_add(1);
            }
        }
        let mut new_behavior_edges = 0_u64;
        for edge in trace
            .edges()
            .map_err(|err| BusError::Runtime(err.to_string()))?
            .into_iter()
            .filter(|edge| edge.src != edge.dst)
        {
            let action = serde_json::to_string(&edge.action)
                .map_err(|err| BusError::Runtime(err.to_string()))?;
            if !store
                .has_behavior_edge(&edge.src, &edge.dst, &action)
                .map_err(|err| BusError::Store(err.to_string()))?
            {
                new_behavior_edges = new_behavior_edges.saturating_add(1);
            }
        }
        let linked_obligations = trace
            .obligations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let mut new_obligations = Vec::new();
        for obligation in &linked_obligations {
            if !store
                .has_behavior_obligation(obligation)
                .map_err(|err| BusError::Store(err.to_string()))?
            {
                new_obligations.push(obligation.clone());
            }
        }
        let api_operations = api_operations.into_iter().collect::<Vec<_>>();
        let mut new_api_operations = Vec::new();
        for operation in &api_operations {
            if !store
                .has_behavior_api_operation(operation)
                .map_err(|err| BusError::Store(err.to_string()))?
            {
                new_api_operations.push(operation.clone());
            }
        }
        let useful = new_behavior_states != 0
            || new_behavior_edges != 0
            || !new_obligations.is_empty()
            || !new_api_operations.is_empty();
        if !useful {
            return Ok(RecordReply {
                session_id,
                change: compiled.change,
                revision: before.to_string(),
                captured_events: u64::try_from(trace.events.len()).unwrap_or(u64::MAX),
                useful: false,
                discarded: true,
                discard_reason: Some("no_new_behavior_or_protection".into()),
                new_behavior_states: 0,
                new_behavior_edges: 0,
                linked_obligations,
                new_obligations,
                api_operations,
                new_api_operations,
                limitations: recording.limitations,
                candidate: None,
                preview: None,
                trace_handle: None,
                network_profile_handle: None,
                runtime_llm_tokens: 0,
            });
        }

        let (candidate, preview) = if trace.obligations.is_empty() {
            (None, None)
        } else {
            let program_id = ProgramId::new(format!(
                "recorded-{}",
                &sha256_hex(session_id.as_bytes())[..16]
            ))
            .map_err(|err| BusError::Identity(err.to_string()))?;
            let program =
                promote(&trace, program_id).map_err(|err| BusError::Runtime(err.to_string()))?;
            let candidate =
                serde_json::to_value(&program).map_err(|err| BusError::Runtime(err.to_string()))?;
            let preview = self.author_preview_controlled(
                &AuthorPreviewCommand {
                    change: compiled.change.clone(),
                    base: cmd.base.clone(),
                    head: cmd.head.clone(),
                    program: candidate.clone(),
                    screenshot: false,
                    trace: false,
                },
                Arc::clone(&cancel),
            )?;
            (Some(candidate), Some(preview))
        };

        let mut event_rows = Vec::new();
        for event in &trace.events {
            let action = serde_json::to_string(&event.action)
                .map_err(|err| BusError::Runtime(err.to_string()))?;
            let digest = event
                .after
                .digest()
                .map_err(|err| BusError::Runtime(err.to_string()))?;
            event_rows.push((action, digest));
        }
        for state in
            std::iter::once(&trace.initial).chain(trace.events.iter().map(|event| &event.after))
        {
            let body = state
                .canonical_json()
                .map_err(|err| BusError::Runtime(err.to_string()))?;
            let digest = state
                .digest()
                .map_err(|err| BusError::Runtime(err.to_string()))?;
            store
                .put_behavior_state(&digest, &body)
                .map_err(|err| BusError::Store(err.to_string()))?;
        }
        for edge in trace
            .edges()
            .map_err(|err| BusError::Runtime(err.to_string()))?
        {
            let action = serde_json::to_string(&edge.action)
                .map_err(|err| BusError::Runtime(err.to_string()))?;
            store
                .put_behavior_edge(&edge.src, &edge.dst, &action)
                .map_err(|err| BusError::Store(err.to_string()))?;
        }
        let trace_body =
            serde_json::to_vec(&trace).map_err(|err| BusError::Runtime(err.to_string()))?;
        let preview_id = preview.as_ref().map(|preview| preview.preview_id.as_str());
        store
            .put_recorded_session(
                &session_id,
                trace.seed,
                trace.fixture.as_deref(),
                before.as_str(),
                preview_id,
                &trace_body,
                &event_rows,
                &linked_obligations,
                &api_operations,
            )
            .map_err(|err| BusError::Store(err.to_string()))?;
        let trace_handle = format!("artifact-session-{session_id}-trace");
        let trace_artifact =
            ArtifactId::new(&trace_handle).map_err(|err| BusError::Identity(err.to_string()))?;
        store
            .put_artifact(&trace_artifact, "behavior-trace", &trace_body)
            .map_err(|err| BusError::Store(err.to_string()))?;
        let network_profile_handle = recording
            .network_profile
            .as_ref()
            .filter(|profile| !profile.entries.is_empty())
            .map(|profile| {
                let handle = format!("artifact-session-{session_id}-network");
                let artifact =
                    ArtifactId::new(&handle).map_err(|err| BusError::Identity(err.to_string()))?;
                let body = serde_json::to_vec(profile)
                    .map_err(|err| BusError::Runtime(err.to_string()))?;
                store
                    .put_artifact(&artifact, "network-replay-profile", &body)
                    .map_err(|err| BusError::Store(err.to_string()))?;
                Ok::<_, BusError>(handle)
            })
            .transpose()?;
        Ok(RecordReply {
            session_id,
            change: compiled.change,
            revision: before.to_string(),
            captured_events: u64::try_from(trace.events.len()).unwrap_or(u64::MAX),
            useful: true,
            discarded: false,
            discard_reason: None,
            new_behavior_states,
            new_behavior_edges,
            linked_obligations,
            new_obligations,
            api_operations,
            new_api_operations,
            limitations: recording.limitations,
            candidate,
            preview,
            trace_handle: Some(trace_handle),
            network_profile_handle,
            runtime_llm_tokens: 0,
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
                network: policy.network,
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

#[cfg(test)]
mod tests;
