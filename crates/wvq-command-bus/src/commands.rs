//! Transport-agnostic commands. MCP and CLI map onto these types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wvq_runtime::{Target, WaitCondition};

fn default_change() -> String {
    "current".to_owned()
}

fn default_purpose() -> String {
    "implementation".to_owned()
}

fn default_token_budget() -> u64 {
    4_000
}

fn default_scope() -> String {
    "impacted".to_owned()
}

fn default_evidence_policy() -> String {
    "standard".to_owned()
}

fn default_base() -> String {
    "HEAD".to_owned()
}

fn default_head() -> String {
    "WORKTREE".to_owned()
}

fn default_authoring_token_budget() -> u64 {
    8_000
}

fn default_true() -> bool {
    true
}

fn default_record_route() -> String {
    "/".to_owned()
}

fn default_record_idle_timeout_ms() -> u64 {
    3_000
}

fn default_record_max_events() -> u32 {
    200
}

/// Bounded context packet for an agent or CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextCommand {
    /// Change id, or `current`.
    #[serde(default = "default_change")]
    pub change: String,
    /// `spec`, `implementation`, or `review`.
    #[serde(default = "default_purpose")]
    pub purpose: String,
    /// Approximate token budget for the reply.
    #[serde(default = "default_token_budget")]
    pub token_budget: u64,
}

/// Plan without executing runners.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanCommand {
    /// Change id, or `current`.
    #[serde(default = "default_change")]
    pub change: String,
}

/// Execute a registered, bounded plan. Never an arbitrary shell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunCommand {
    /// Change id, or `current`.
    #[serde(default = "default_change")]
    pub change: String,
    /// `impacted` or `all`.
    #[serde(default = "default_scope")]
    pub scope: String,
    /// `standard`, `minimal`, or `none`.
    #[serde(default = "default_evidence_policy")]
    pub evidence_policy: String,
    /// Immutable Git base revision. Defaults to `HEAD` for a working-tree run.
    #[serde(default = "default_base")]
    pub base: String,
    /// `WORKTREE` or a commit-ish that must resolve to the checked-out clean `HEAD`.
    #[serde(default = "default_head")]
    pub head: String,
}

/// Compact progress for a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusCommand {
    /// Run identity. Omit for the latest run.
    #[serde(default)]
    pub run_id: Option<String>,
}

/// Assemble a multi-axis quality verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyCommand {
    /// Change id, or `current`.
    #[serde(default = "default_change")]
    pub change: String,
}

/// Explain one finding, proof, selection, or obligation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplainCommand {
    /// Identity to explain.
    pub id: String,
}

/// Fetch evidence metadata; large bytes stay behind a handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceCommand {
    /// CAS handle or artifact id.
    pub handle: String,
}

/// Validate `OpenSpec` + `quality.yaml` without sealing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecCommand {
    /// Change id, or `current`.
    #[serde(default = "default_change")]
    pub change: String,
}

/// Alias for CLI `wvq analyze` (same payload as [`ContextCommand`]).
pub type AnalyzeCommand = ContextCommand;

/// Quality Debt Ratchet summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebtCommand {
    /// Change id, or `current`.
    #[serde(default = "default_change")]
    pub change: String,
    /// Immutable Git base revision.
    #[serde(default = "default_base")]
    pub base: String,
    /// `WORKTREE` or the checked-out clean head commit.
    #[serde(default = "default_head")]
    pub head: String,
}

/// Minimal impacted selection. Does not execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectCommand {
    /// Change id, or `current`.
    #[serde(default = "default_change")]
    pub change: String,
    /// Immutable Git base revision.
    #[serde(default = "default_base")]
    pub base: String,
    /// `WORKTREE` or the checked-out clean head commit.
    #[serde(default = "default_head")]
    pub head: String,
}

/// Explicit opt-in local model escape. Never used by normal verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCommand {
    /// Change whose AI budget is charged.
    #[serde(default = "default_change")]
    pub change: String,
    /// `planning`, `runtime`, `browser_escape`, or `vision`.
    pub kind: String,
    /// Bounded packet sent to the configured loopback model.
    pub prompt: String,
}

/// Build a bounded, revision-bound packet for authoring a Playwright-backed `TestProgram`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorDraftCommand {
    /// Change id, or `current`.
    #[serde(default = "default_change")]
    pub change: String,
    /// Immutable Git base revision.
    #[serde(default = "default_base")]
    pub base: String,
    /// `WORKTREE` or the checked-out clean head commit.
    #[serde(default = "default_head")]
    pub head: String,
    /// Approximate token budget for intent and graph context.
    #[serde(default = "default_authoring_token_budget")]
    pub token_budget: u64,
    /// Explicitly spend the configured planning-model budget to propose a candidate.
    #[serde(default)]
    pub use_model: bool,
}

/// Strictly validate an agent-authored `TestProgram` against sealed obligations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorValidateCommand {
    /// Change id, or `current`.
    #[serde(default = "default_change")]
    pub change: String,
    /// Candidate canonical `TestProgram` JSON object.
    pub program: serde_json::Value,
}

/// Execute a validated candidate through the configured Playwright runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorPreviewCommand {
    /// Change id, or `current`.
    #[serde(default = "default_change")]
    pub change: String,
    /// Immutable Git base revision.
    #[serde(default = "default_base")]
    pub base: String,
    /// `WORKTREE` or the checked-out clean head commit.
    #[serde(default = "default_head")]
    pub head: String,
    /// Candidate canonical `TestProgram` JSON object.
    pub program: serde_json::Value,
    /// Capture screenshots on every attempted step.
    #[serde(default = "default_true")]
    pub screenshot: bool,
    /// Capture a Playwright trace for the preview.
    #[serde(default)]
    pub trace: bool,
}

/// Passively record natural app use and retain only new regression knowledge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordCommand {
    /// Change id, or `current`.
    #[serde(default = "default_change")]
    pub change: String,
    /// Immutable Git base revision.
    #[serde(default = "default_base")]
    pub base: String,
    /// `WORKTREE` or the checked-out clean head commit.
    #[serde(default = "default_head")]
    pub head: String,
    /// Root-relative same-origin route opened for the session.
    #[serde(default = "default_record_route")]
    pub route: String,
    /// Explicit safe replay fixtures. Unknown form values are redacted and not recorded.
    #[serde(default)]
    pub fixture_values: BTreeMap<String, String>,
    /// Inactivity that ends a session without requiring a recorder-specific UI.
    #[serde(default = "default_record_idle_timeout_ms")]
    pub idle_timeout_ms: u64,
    /// Hard semantic-event ceiling.
    #[serde(default = "default_record_max_events")]
    pub max_events: u32,
    /// Override repository browser visibility. Defaults to a visible browser.
    #[serde(default)]
    pub headless: Option<bool>,
}

/// Promote one passing, same-revision preview into a persisted `TestProgram`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorPromoteCommand {
    /// Change id, or `current`.
    #[serde(default = "default_change")]
    pub change: String,
    /// Preview identity returned by [`AuthorPreviewCommand`].
    pub preview_id: String,
    /// Exact canonical program exercised by that preview.
    pub program: serde_json::Value,
}

/// One strictly bounded repair proposed for a persisted browser program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "edit", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthorHealEdit {
    /// Add a semantic alias to an activate/fill/select target.
    Retarget {
        /// Zero-based step index.
        step: usize,
        /// Recovered semantic target. `XPath` is not representable.
        target: Target,
    },
    /// Insert a deterministic typed wait after one existing step.
    InsertWait {
        /// Zero-based step index.
        after: usize,
        /// Typed wait predicate.
        condition: WaitCondition,
    },
}

/// Repair and replay the latest persisted browser program under the same seal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorHealCommand {
    /// Change id, or `current`.
    #[serde(default = "default_change")]
    pub change: String,
    /// Immutable Git base revision.
    #[serde(default = "default_base")]
    pub base: String,
    /// `WORKTREE` or the checked-out clean head commit.
    #[serde(default = "default_head")]
    pub head: String,
    /// Persisted program identity.
    pub program_id: String,
    /// Optimistic-concurrency version expected by the caller.
    pub expected_program_revision: u32,
    /// Locator/wait edits only.
    pub edits: Vec<AuthorHealEdit>,
    /// Capture screenshots during the proving replay.
    #[serde(default = "default_true")]
    pub screenshot: bool,
    /// Capture a Playwright trace during the proving replay.
    #[serde(default)]
    pub trace: bool,
}

/// List known `OpenSpec` changes. Studio uses this for its Changes screen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangesCommand {}

/// Build one bounded changed-intent recovery packet. Never seals a candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryCommand {
    /// `OpenSpec` change identity.
    #[serde(default = "default_change")]
    pub change: String,
    /// Immutable Git base revision.
    #[serde(default = "default_base")]
    pub base: String,
    /// Working tree or checked-out clean head revision.
    #[serde(default = "default_head")]
    pub head: String,
}

/// Scaffold `.weavatrix-quality/` without inventing tests, browser, or a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitCommand {
    /// Overwrite an existing policy. Default false.
    #[serde(default)]
    pub force: bool,
}

/// Every bus command. HTTP/CLI/MCP share this enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// [`ContextCommand`].
    Context(ContextCommand),
    /// [`PlanCommand`].
    Plan(PlanCommand),
    /// [`RunCommand`].
    Run(RunCommand),
    /// [`StatusCommand`].
    Status(StatusCommand),
    /// [`VerifyCommand`].
    Verify(VerifyCommand),
    /// [`ExplainCommand`].
    Explain(ExplainCommand),
    /// [`EvidenceCommand`].
    Evidence(EvidenceCommand),
    /// `wvq spec validate`.
    SpecValidate(SpecCommand),
    /// `wvq spec seal`.
    SpecSeal(SpecCommand),
    /// `wvq analyze`.
    Analyze(AnalyzeCommand),
    /// `wvq debt`.
    Debt(DebtCommand),
    /// `wvq select`.
    Select(SelectCommand),
    /// [`ModelCommand`].
    Model(ModelCommand),
    /// [`AuthorDraftCommand`].
    AuthorDraft(AuthorDraftCommand),
    /// [`AuthorValidateCommand`].
    AuthorValidate(AuthorValidateCommand),
    /// [`AuthorPreviewCommand`].
    AuthorPreview(AuthorPreviewCommand),
    /// [`RecordCommand`].
    Record(RecordCommand),
    /// [`AuthorPromoteCommand`].
    AuthorPromote(AuthorPromoteCommand),
    /// [`AuthorHealCommand`].
    AuthorHeal(AuthorHealCommand),
    /// [`ChangesCommand`].
    Changes(ChangesCommand),
    /// [`RecoveryCommand`].
    Recovery(RecoveryCommand),
    /// [`InitCommand`].
    Init(InitCommand),
}

impl Command {
    /// Stable command name for logs and tagged JSON.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Context(_) => "context",
            Self::Plan(_) => "plan",
            Self::Run(_) => "run",
            Self::Status(_) => "status",
            Self::Verify(_) => "verify",
            Self::Explain(_) => "explain",
            Self::Evidence(_) => "evidence",
            Self::SpecValidate(_) => "spec_validate",
            Self::SpecSeal(_) => "spec_seal",
            Self::Analyze(_) => "analyze",
            Self::Debt(_) => "debt",
            Self::Select(_) => "select",
            Self::Model(_) => "model",
            Self::AuthorDraft(_) => "author_draft",
            Self::AuthorValidate(_) => "author_validate",
            Self::AuthorPreview(_) => "author_preview",
            Self::Record(_) => "record",
            Self::AuthorPromote(_) => "author_promote",
            Self::AuthorHeal(_) => "author_heal",
            Self::Changes(_) => "changes",
            Self::Recovery(_) => "recovery",
            Self::Init(_) => "init",
        }
    }
}
