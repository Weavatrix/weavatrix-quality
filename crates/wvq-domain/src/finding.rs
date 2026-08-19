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

/// Order-independent identity of one debt item.
///
/// Built from [`QualityFinding::check`] + canonical [`SubjectRef`]. Summary,
/// severity, and current [`FindingState`] are not part of the fingerprint, so
/// wording changes cannot mint a new debt identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DebtFingerprint {
    /// Check catalogue id.
    pub check: CheckId,
    /// Subject kind (`file`, `symbol`, …).
    pub subject_kind: String,
    /// Canonical subject value (`/` path separators).
    pub subject_value: String,
}

impl QualityFinding {
    /// Stable ratchet fingerprint. Independent of input order and summary text.
    #[must_use]
    pub fn fingerprint(&self) -> DebtFingerprint {
        let (subject_kind, subject_value) = subject_canonical(&self.subject);
        DebtFingerprint {
            check: self.check.clone(),
            subject_kind,
            subject_value,
        }
    }

    /// Copy with a classified ratchet state.
    #[must_use]
    pub fn with_state(mut self, state: FindingState) -> Self {
        self.state = state;
        self
    }
}

impl std::fmt::Display for DebtFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}",
            self.check, self.subject_kind, self.subject_value
        )
    }
}

fn subject_canonical(subject: &SubjectRef) -> (String, String) {
    match subject {
        SubjectRef::File(path) => ("file".into(), path.replace('\\', "/")),
        SubjectRef::Symbol(name) => ("symbol".into(), name.clone()),
        SubjectRef::Endpoint(name) => ("endpoint".into(), name.clone()),
        SubjectRef::Test(name) => ("test".into(), name.clone()),
        SubjectRef::GraphNode(name) => ("graph_node".into(), name.clone()),
        SubjectRef::Obligation(id) => ("obligation".into(), id.to_string()),
        SubjectRef::Requirement(id) => ("requirement".into(), id.to_string()),
        SubjectRef::Change(id) => ("change".into(), id.to_string()),
    }
}
