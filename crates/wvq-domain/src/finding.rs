//! Quality findings and debt-ratchet state.

use crate::ids::{ChangeId, CheckId, ObligationId, RequirementId};
use serde::{Deserialize, Serialize};

/// Policy-facing severity of a quality finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Advisory, no gate by default.
    Info,
    /// Visible; may warn without blocking.
    Warn,
    /// Blocking under default PR policy.
    Error,
}

/// Debt-ratchet state for a finding. Matches spec §9 `DebtState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingState {
    /// Present in the baseline; not newly blamed on this change.
    Existing,
    /// Introduced by this change.
    New,
    /// Present in baseline and gone on head.
    Fixed,
    /// Previously fixed fingerprint reappears.
    Returned,
    /// Explicit, provenance-bearing exception.
    Excepted,
    /// Warn-severity finding that is not yet blocking debt.
    Warning,
    /// Approaching a configured budget (LOC, cycles, …).
    ApproachingBudget,
}

/// Spec name for the same ratchet classification.
pub type DebtState = FindingState;

/// What a finding is about. Values stay revision-bound strings or typed IDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SubjectRef {
    /// Repository-relative file path.
    File(String),
    /// Symbol / graph node name.
    Symbol(String),
    /// Externally visible API or route.
    Endpoint(String),
    /// Test case or program identity as reported by a runner.
    Test(String),
    /// Graph node identifier from Weavatrix (not a second graph).
    GraphNode(String),
    /// Sealed obligation.
    Obligation(ObligationId),
    /// `OpenSpec` requirement.
    Requirement(RequirementId),
    /// `OpenSpec` / quality change.
    Change(ChangeId),
}

/// One quality finding. Evidence details stay with later check crates.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QualityFinding {
    /// Catalogue identity (`WVQ-DEAD-001`).
    pub check: CheckId,
    /// Gate severity.
    pub severity: Severity,
    /// Ratchet classification.
    pub state: FindingState,
    /// Subject the finding attaches to.
    pub subject: SubjectRef,
    /// Short human-readable explanation. Not a verdict percentage.
    pub summary: String,
}
