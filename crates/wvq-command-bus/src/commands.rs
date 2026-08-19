//! Transport-agnostic commands. MCP and CLI map onto these types.

use serde::{Deserialize, Serialize};

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
}

/// Minimal impacted selection. Does not execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectCommand {
    /// Change id, or `current`.
    #[serde(default = "default_change")]
    pub change: String,
}

/// List known `OpenSpec` changes. Studio uses this for its Changes screen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangesCommand {}

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
    /// [`ChangesCommand`].
    Changes(ChangesCommand),
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
            Self::Changes(_) => "changes",
        }
    }
}
