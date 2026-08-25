//! Bounded replies. Large artifacts are handles, never dumped into context.

use serde::Serialize;
use serde_json::Value;
use wvq_proof::ChangeQualityVerdict;
use wvq_spec_recovery::{Questions, RecoveryPacket, ReviewSnapshot};

/// Maximum UTF-8 bytes allowed inline in [`EvidenceReply`]. Larger stays a handle.
pub const INLINE_LIMIT: usize = 4_096;

/// Bounded recovery output shared by the ordinary CLI and other transports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecoveryReply {
    /// Revision-bound evidence packet prepared without a model.
    pub packet: RecoveryPacket,
    /// One candidate at a time, after deterministic checks.
    pub review: ReviewSnapshot,
    /// Narrow questions that remain for intent owners.
    pub questions: Questions,
    /// Proposed `OpenSpec` patch. Never presented as sealed.
    pub proposed_patch: String,
    /// Normal deterministic preparation spends no runtime model tokens.
    pub runtime_llm_tokens: u64,
}

/// Approximate LLM tokens: four Unicode scalars per token, rounded up.
#[must_use]
pub fn estimate_tokens(text: &str) -> u64 {
    let chars = text.chars().count() as u64;
    if chars == 0 { 0 } else { chars.div_ceil(4) }
}

/// Keep `items` under `budget` tokens. Sets `truncated` when anything is dropped.
#[must_use]
pub fn bound_items(items: Vec<String>, budget: u64) -> (Vec<String>, u64, bool) {
    let mut out = Vec::new();
    let mut used = 0_u64;
    let mut truncated = false;
    for item in items {
        let cost = estimate_tokens(&item).max(1);
        if used.saturating_add(cost) > budget {
            truncated = true;
            if out.is_empty() {
                let prefix = truncate_to_budget(&item, budget);
                used = estimate_tokens(&prefix);
                out.push(prefix);
            }
            break;
        }
        used = used.saturating_add(cost);
        out.push(item);
    }
    (out, used, truncated)
}

fn truncate_to_budget(text: &str, budget: u64) -> String {
    let mut used = 0_u64;
    let mut end = 0;
    for (idx, ch) in text.char_indices() {
        let next = estimate_tokens(ch.encode_utf8(&mut [0; 4])).max(1);
        if used.saturating_add(next) > budget {
            break;
        }
        used = used.saturating_add(next);
        end = idx + ch.len_utf8();
    }
    let mut prefix = text[..end].to_owned();
    prefix.push('…');
    prefix
}

/// Bounded `QualityContextPacket`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextReply {
    /// Change that was resolved.
    pub change: String,
    /// Requested purpose.
    pub purpose: String,
    /// Neighbouring requirements (truncated to budget).
    pub requirements: Vec<String>,
    /// Compiled obligations.
    pub obligations: Vec<String>,
    /// Deterministic quality heuristics.
    pub heuristics: Vec<String>,
    /// Coverage notes. Unmeasured is not uncovered.
    pub coverage: Vec<String>,
    /// Whether anything was dropped to honour the budget.
    pub truncated: bool,
    /// Approximate tokens in this packet.
    pub tokens_used: u64,
    /// Budget that was applied.
    pub token_budget: u64,
}

/// Plan: requirements, obligations, risk, proofs, gaps. Never execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanReply {
    /// Change.
    pub change: String,
    /// Requirement identities.
    pub requirements: Vec<String>,
    /// Obligation identities and kinds.
    pub obligations: Vec<String>,
    /// Named risk evidence. Never an opaque percentage.
    pub risk: Vec<String>,
    /// Existing proof ids, if any.
    pub existing_proofs: Vec<String>,
    /// Unproven / missing-evidence gaps.
    pub gaps: Vec<String>,
    /// Deterministic check families that will run later.
    pub checks: Vec<String>,
    /// Always false. Plan does not execute.
    pub executed: bool,
}

/// Run acknowledgement with handles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunReply {
    /// Run identity.
    pub run_id: String,
    /// Change.
    pub change: String,
    /// Requested immutable Git base.
    pub base: String,
    /// Requested head (`WORKTREE` or the checked-out commit-ish).
    pub head: String,
    /// Exact commit resolved from the requested base ref.
    pub base_commit: String,
    /// Exact checked-out commit underlying the head/worktree.
    pub head_commit: String,
    /// Exact common ancestor used for all base/head deltas.
    pub merge_base: String,
    /// Scope requested by the caller.
    pub requested_scope: String,
    /// Effective scope. May widen from `impacted` to `all` when complete
    /// selection evidence is unavailable.
    pub scope: String,
    /// Exact reason the requested scope was kept or widened.
    pub scope_reason: String,
    /// `complete` or `queued`.
    pub status: String,
    /// True when a run was recorded (still no arbitrary shell).
    pub executed: bool,
    /// Aggregate executor outcome: `passed`, `failed`, or `error`.
    pub outcome: String,
    /// Number of repository test paths selected for this run.
    pub selected_test_count: u64,
    /// Number of filterable repository test paths available to this run.
    pub available_test_count: u64,
    /// Number of bounded registered executor processes invoked.
    pub executor_invocations: u64,
    /// Number of typed browser programs invoked.
    pub browser_programs: u64,
    /// Unique normalized browser states contributed by this run.
    pub behavior_state_count: u64,
    /// Browser states first admitted to the persistent `BehaviorGraph`.
    pub new_behavior_state_count: u64,
    /// Unique normalized browser transitions contributed by this run.
    pub behavior_edge_count: u64,
    /// Browser transitions first admitted to the persistent `BehaviorGraph`.
    pub new_behavior_edge_count: u64,
    /// Number of normalized test-case occurrences recorded for history.
    pub recorded_test_count: u64,
    /// Number of failed/error test-case occurrences in this run.
    pub failed_test_count: u64,
    /// Number of current test identities with both pass and fail/error history.
    pub flaky_test_count: u64,
    /// Failures not classified by deterministic evidence.
    pub unknown_failure_count: u64,
    /// Artifact handles produced (possibly empty).
    pub artifact_handles: Vec<String>,
}

/// Result of one defensive impacted-vs-full selection audit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectionAuditReply {
    /// Stable audit identity.
    pub audit_id: String,
    /// `corroborated`, `contradicted`, `unmeasured`, or `not_reduced`.
    pub status: String,
    /// Fail/error identities present only in the full run.
    pub missed_failure_count: u64,
    /// Safely resolved missed test paths fed back into future selection.
    pub learned_test_count: u64,
    /// CAS-backed audit artifact attached to the full run, when persisted.
    pub evidence_handle: Option<String>,
}

/// Compact progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusReply {
    /// Latest or requested run.
    pub run_id: Option<String>,
    /// `idle`, `queued`, or `complete`.
    pub status: String,
    /// Aggregate executor outcome when a run exists.
    pub outcome: Option<String>,
    /// Handles currently known.
    pub handles: Vec<String>,
}

/// Read-only Application Surface Graph projection for MCP and Studio.
///
/// `protected` is fully measured and hit. `partial` is mixed. `unmeasured`
/// covers both absent reports and instrumented zeros — Studio never calls
/// missing coverage "uncovered". A missing artifact is [`Self::absent`], not
/// an empty clean graph. This view is never a gate.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct ApplicationSurfaceView {
    /// False when the run never stored a surface graph.
    pub present: bool,
    /// True when the Weavatrix projection hit its surface ceiling.
    pub truncated: bool,
    /// Surfaces whose every implementation node was hit.
    pub protected: Vec<String>,
    /// Surfaces with at least one hit and at least one gap.
    pub partial: Vec<String>,
    /// Surfaces with no complete measured report.
    pub unmeasured: Vec<String>,
}

impl ApplicationSurfaceView {
    /// No artifact. Missing evidence is not an empty surface list.
    #[must_use]
    pub fn absent() -> Self {
        Self::default()
    }
}

/// Read-only Surface Evidence Matrix for MCP and Studio.
///
/// Each cell is `present`, `absent`, or `unmeasured`. A missing artifact is
/// [`Self::absent`], not a table of empty absents. This view is never a gate.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct SurfaceEvidenceMatrixView {
    /// False when the run never stored a matrix.
    pub present: bool,
    /// True when the surface graph or a binding reach was truncated.
    pub truncated: bool,
    /// One row per named production surface.
    pub surfaces: Vec<wvq_intelligence::SurfaceEvidenceRow>,
}

impl SurfaceEvidenceMatrixView {
    /// No artifact. Missing evidence is not an empty matrix.
    #[must_use]
    pub fn absent() -> Self {
        Self::default()
    }
}

/// Multi-axis verdict. Not a quality percentage.
///
/// `verdict` stays the combined [`wvq_proof::ProofVerdict`] token so existing
/// callers keep working, but `blocking` and the process exit code now come from
/// the composite [`ChangeQualityVerdict`]: a change can prove every obligation
/// and still be blocked by a lost protection net or a new UI regression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerifyReply {
    /// Change.
    pub change: String,
    /// Combined `Proof` verdict token.
    pub verdict: String,
    /// True when the composite verdict must fail CI.
    pub blocking: bool,
    /// Per-obligation proof summaries.
    pub proofs: Vec<ProofSummary>,
    /// Composite state token (`BLOCKED`, `PASS_WITH_WARNINGS`, …).
    pub state: String,
    /// Every measured axis with its own facts and provenance.
    pub quality: ChangeQualityVerdict,
    /// Read-only Application Surface Graph projection. Never a gate.
    pub application_surface: ApplicationSurfaceView,
    /// Read-only Surface Evidence Matrix. Never a gate.
    pub surface_evidence: SurfaceEvidenceMatrixView,
}

impl VerifyReply {
    /// Process exit code: `0` clean, `2` blocking, `1` otherwise.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        self.quality.exit_code()
    }
}

/// One assembled proof, compact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProofSummary {
    /// Proof id.
    pub id: String,
    /// Requirement the obligation belongs to. Drives Studio drill-down.
    pub requirement: String,
    /// Obligation.
    pub obligation: String,
    /// Verdict token.
    pub verdict: String,
}

impl ProofSummary {
    /// Whether this proof is ordinary pass-noise the dashboard should hide.
    #[must_use]
    pub fn is_passing(&self) -> bool {
        self.verdict == "PROVEN"
    }
}

/// Known `OpenSpec` changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangesReply {
    /// Change identities, sorted.
    pub changes: Vec<String>,
}

/// Provenance-bearing explanation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainReply {
    /// Requested id.
    pub id: String,
    /// `obligation`, `proof`, `run`, `finding`, or `selection`.
    pub kind: String,
    /// Short summary.
    pub summary: String,
    /// File/line or selection chain.
    pub provenance: Vec<String>,
}

/// Bounded evidence. `inline_text` is absent for large or binary artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceReply {
    /// Handle the caller used or was given.
    pub handle: String,
    /// Artifact kind (`text`, `junit`, `screenshot`, …).
    pub kind: String,
    /// Byte length of the stored blob.
    pub byte_len: u64,
    /// Content hash when known.
    pub content_hash: Option<String>,
    /// Small UTF-8 payload only.
    pub inline_text: Option<String>,
}

/// `wvq spec validate` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpecValidateReply {
    /// Change.
    pub change: String,
    /// Requirement count.
    pub requirements: u64,
    /// Compiled obligation count.
    pub obligations: u64,
    /// Always true on success (errors are [`crate::BusError`]).
    pub ok: bool,
}

/// `wvq spec seal` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpecSealReply {
    /// Change.
    pub change: String,
    /// `OracleSeal` id.
    pub seal_id: String,
    /// Canonical digest.
    pub digest: String,
    /// Obligation count sealed.
    pub obligations: u64,
}

/// Alias of [`ContextReply`] for `wvq analyze`.
pub type AnalyzeReply = ContextReply;

/// Debt-ratchet bucket counts. Empty when no findings were supplied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DebtReply {
    /// Exact Weavatrix revision used for the audit, when live.
    pub revision: Option<String>,
    /// Requested immutable Git base.
    pub base: String,
    /// Requested head (`WORKTREE` or the checked-out commit-ish).
    pub head: String,
    /// Whether immutable base/head comparison was available.
    pub comparison_present: bool,
    /// Present on base and head.
    pub existing: u64,
    /// Introduced on head.
    pub new: u64,
    /// Gone on head.
    pub fixed: u64,
    /// Previously fixed, back on head.
    pub returned: u64,
    /// Explicit exceptions.
    pub excepted: u64,
    /// Compact finding summaries.
    pub findings: Vec<String>,
    /// Explicitly unmeasured debt axes.
    pub limitations: Vec<String>,
}

/// Minimal selection. Does not execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectReply {
    /// Exact Weavatrix revision used for selection, when live.
    pub revision: Option<String>,
    /// Requested immutable Git base.
    pub base: String,
    /// Requested head (`WORKTREE` or the checked-out commit-ish).
    pub head: String,
    /// Algorithm id.
    pub algorithm: String,
    /// Selected test ids.
    pub selected: Vec<String>,
    /// High-risk obligations no candidate covered.
    pub uncovered_mandatory: Vec<String>,
    /// Explanation chains, aligned with `selected`.
    pub explanations: Vec<Vec<String>>,
    /// Always false.
    pub executed: bool,
    /// True only when every mandatory obligation has a mapped candidate.
    pub selection_complete: bool,
}

/// One explicitly requested, budgeted loopback model decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelReply {
    /// Change whose budget was charged.
    pub change: String,
    /// Budget axis.
    pub kind: String,
    /// Local model identity.
    pub model: String,
    /// Assistant text.
    pub text: String,
    /// Measured input tokens.
    pub input_tokens: u64,
    /// Measured output tokens.
    pub output_tokens: u64,
    /// Calculated cost in micros.
    pub cost_micros: u64,
}

/// One sealed obligation exposed to the authoring agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthoringObligation {
    /// Obligation identity accepted by `assert` actions.
    pub id: String,
    /// Requirement identity.
    pub requirement: String,
    /// Scenario identity.
    pub scenario: String,
    /// Obligation kind.
    pub kind: String,
    /// Consequence weight.
    pub risk: String,
    /// Optional sealed precondition.
    pub condition: Option<Value>,
    /// Sealed expected predicate. `None` means this obligation is not browser-previewable.
    pub expected: Option<Value>,
    /// Evidence required by policy.
    pub required_evidence: Vec<String>,
}

/// Revision-bound inputs for producing a Playwright-backed `TestProgram`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthorDraftReply {
    /// Resolved change.
    pub change: String,
    /// Exact Weavatrix revision.
    pub revision: String,
    /// Immutable Git base.
    pub base: String,
    /// Requested head.
    pub head: String,
    /// Changed paths, including removed paths.
    pub changed_files: Vec<String>,
    /// Bounded requirement and graph context.
    pub context: Vec<String>,
    /// Complete sealed authority; never truncated.
    pub obligations: Vec<AuthoringObligation>,
    /// True only when non-authoritative context was dropped.
    pub truncated: bool,
    /// Approximate inline tokens used.
    pub tokens_used: u64,
    /// Requested packet budget.
    pub token_budget: u64,
    /// Optional model-proposed candidate. Never persisted or sealed.
    pub candidate: Option<Value>,
    /// Usage for the explicit model call, when requested.
    pub model_usage: Option<AuthorModelUsage>,
}

/// Compact, measured usage for an explicit authoring model call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthorModelUsage {
    /// Local model identity.
    pub model: String,
    /// Measured input tokens.
    pub input_tokens: u64,
    /// Measured output tokens.
    pub output_tokens: u64,
    /// Calculated cost in micros.
    pub cost_micros: u64,
}

/// Strict validation result for an authoring candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthorValidateReply {
    /// Resolved change.
    pub change: String,
    /// Existing `OracleSeal` that supplied the predicates.
    pub seal_id: String,
    /// Candidate program id.
    pub program_id: String,
    /// Validated canonical program.
    pub program: Value,
    /// Obligations the program asserts.
    pub obligations: Vec<String>,
    /// Always true on success.
    pub valid: bool,
    /// Always false; validation never writes source or intent.
    pub persisted: bool,
}

/// Evidence handles from one explicit Playwright preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthorPreviewReply {
    /// Stable admission identity required for explicit promotion.
    pub preview_id: String,
    /// Resolved change.
    pub change: String,
    /// Exact Weavatrix revision checked before and after execution.
    pub revision: String,
    /// Candidate program id.
    pub program_id: String,
    /// All actions and sealed assertions passed.
    pub passed: bool,
    /// Successfully asserted obligations.
    pub asserted: Vec<String>,
    /// Contradicted sealed obligations.
    pub contradicted: Vec<String>,
    /// Stable browser failure, if any.
    pub failure: Option<String>,
    /// CAS handles for structured observations.
    pub observation_handles: Vec<String>,
    /// CAS handles for screenshots.
    pub screenshot_handles: Vec<String>,
    /// CAS handle for the trace, when requested and produced.
    pub trace_handle: Option<String>,
    /// Always false; the candidate itself is not saved or registered.
    pub program_persisted: bool,
}

/// Result of one passive Playwright recording and deterministic novelty admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecordReply {
    /// Ephemeral or persisted session identity.
    pub session_id: String,
    /// Resolved `OpenSpec` change.
    pub change: String,
    /// Exact repository/Weavatrix revision observed before and after recording.
    pub revision: String,
    /// Semantic events captured, including the initial navigation.
    pub captured_events: u64,
    /// True only when the session adds behavior, API, or obligation knowledge.
    pub useful: bool,
    /// True when no persistent session or candidate was created.
    pub discarded: bool,
    /// Stable reason for a redundant or unusable session.
    pub discard_reason: Option<String>,
    /// Behavior states absent before this session.
    pub new_behavior_states: u64,
    /// Non-loop behavior edges absent before this session.
    pub new_behavior_edges: u64,
    /// Existing sealed obligations that matched the exact final state.
    pub linked_obligations: Vec<String>,
    /// Newly learned obligation links.
    pub new_obligations: Vec<String>,
    /// Normalized API operations observed during the session.
    pub api_operations: Vec<String>,
    /// Newly learned API operations.
    pub new_api_operations: Vec<String>,
    /// Redaction, budget, or replay limitations; never raw form values.
    pub limitations: Vec<String>,
    /// Canonical recorded `TestProgram`, only when at least one sealed oracle matched.
    pub candidate: Option<Value>,
    /// Passing/failing replay admission. Promotion remains explicit and QA-gated.
    pub preview: Option<AuthorPreviewReply>,
    /// CAS-backed canonical `BehaviorTrace` for admitted sessions.
    pub trace_handle: Option<String>,
    /// CAS-backed redacted API response profile, when the session captured one.
    pub network_profile_handle: Option<String>,
    /// Normal recording and replay never spend model tokens.
    pub runtime_llm_tokens: u64,
}

/// Result of explicitly promoting one passing preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthorPromoteReply {
    /// Resolved change.
    pub change: String,
    /// Exact repository revision exercised by the preview.
    pub revision: String,
    /// Existing `OracleSeal`; promotion never creates or changes one.
    pub seal_id: String,
    /// Canonical program identity.
    pub program_id: String,
    /// Persisted program revision (1 for first promotion).
    pub program_revision: u32,
    /// Always true on success.
    pub persisted: bool,
    /// False when the exact preview had already been promoted.
    pub created: bool,
}

/// Result of a safe-healing replay and optional versioned persistence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthorHealReply {
    /// Admission preview created for the repaired candidate.
    pub preview_id: String,
    /// Resolved change.
    pub change: String,
    /// Exact repository revision checked before and after replay.
    pub revision: String,
    /// Unchanged existing `OracleSeal`.
    pub seal_id: String,
    /// Stable program identity.
    pub program_id: String,
    /// Revision the repair started from.
    pub previous_program_revision: u32,
    /// New revision, only when the proving replay passed and persistence succeeded.
    pub program_revision: Option<u32>,
    /// Whether every original sealed assertion passed.
    pub passed: bool,
    /// Successfully asserted obligations.
    pub asserted: Vec<String>,
    /// Contradicted sealed obligations.
    pub contradicted: Vec<String>,
    /// Stable runtime failure, if any.
    pub failure: Option<String>,
    /// CAS handles for structured observations.
    pub observation_handles: Vec<String>,
    /// CAS handles for screenshots.
    pub screenshot_handles: Vec<String>,
    /// CAS handle for the trace, when requested and produced.
    pub trace_handle: Option<String>,
    /// True only when the repaired program became a new version.
    pub persisted: bool,
    /// False when the exact healing preview had already produced that version.
    pub created: bool,
}

/// Files written by `wvq init`. Never a proof or a seal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InitReply {
    /// Repository-relative files created or overwritten.
    pub created: Vec<String>,
    /// Repository-relative files left untouched.
    pub skipped: Vec<String>,
    /// Policy file that `quality_policy_v: 1` now occupies.
    pub config: String,
    /// Scaffolding spends no model tokens.
    pub runtime_llm_tokens: u64,
}

/// Tagged reply for CLI JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "command", content = "body")]
pub enum Reply {
    /// [`ContextReply`].
    #[serde(rename = "context")]
    Context(ContextReply),
    /// [`PlanReply`].
    #[serde(rename = "plan")]
    Plan(PlanReply),
    /// [`RunReply`].
    #[serde(rename = "run")]
    Run(RunReply),
    /// [`StatusReply`].
    #[serde(rename = "status")]
    Status(StatusReply),
    /// [`VerifyReply`]. Boxed: the composite verdict carries every axis, so it
    /// is much larger than the other replies.
    #[serde(rename = "verify")]
    Verify(Box<VerifyReply>),
    /// [`ExplainReply`].
    #[serde(rename = "explain")]
    Explain(ExplainReply),
    /// [`EvidenceReply`].
    #[serde(rename = "evidence")]
    Evidence(EvidenceReply),
    /// [`SpecValidateReply`].
    #[serde(rename = "spec_validate")]
    SpecValidate(SpecValidateReply),
    /// [`SpecSealReply`].
    #[serde(rename = "spec_seal")]
    SpecSeal(SpecSealReply),
    /// [`AnalyzeReply`].
    #[serde(rename = "analyze")]
    Analyze(AnalyzeReply),
    /// [`DebtReply`].
    #[serde(rename = "debt")]
    Debt(DebtReply),
    /// [`SelectReply`].
    #[serde(rename = "select")]
    Select(SelectReply),
    /// [`ModelReply`].
    #[serde(rename = "model")]
    Model(ModelReply),
    /// [`AuthorDraftReply`].
    #[serde(rename = "author_draft")]
    AuthorDraft(AuthorDraftReply),
    /// [`AuthorValidateReply`].
    #[serde(rename = "author_validate")]
    AuthorValidate(AuthorValidateReply),
    /// [`AuthorPreviewReply`].
    #[serde(rename = "author_preview")]
    AuthorPreview(AuthorPreviewReply),
    /// [`RecordReply`].
    #[serde(rename = "record")]
    Record(Box<RecordReply>),
    /// [`AuthorPromoteReply`].
    #[serde(rename = "author_promote")]
    AuthorPromote(AuthorPromoteReply),
    /// [`AuthorHealReply`].
    #[serde(rename = "author_heal")]
    AuthorHeal(AuthorHealReply),
    /// [`ChangesReply`].
    #[serde(rename = "changes")]
    Changes(ChangesReply),
    /// [`RecoveryReply`].
    #[serde(rename = "recovery")]
    Recovery(Box<RecoveryReply>),
    /// [`InitReply`].
    #[serde(rename = "init")]
    Init(InitReply),
}

impl Reply {
    /// Blocking verify verdict, if this is a verify reply.
    #[must_use]
    pub fn verify_exit_code(&self) -> Option<i32> {
        match self {
            Self::Verify(reply) => Some(reply.exit_code()),
            _ => None,
        }
    }
}
