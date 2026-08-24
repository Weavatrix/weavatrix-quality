//! Spec §85–§88 protection projections for MCP and Studio.
//!
//! One view object so both transports answer the same question the same way:
//! *what protected this flow before the change, and what protects it now?*

use serde::{Deserialize, Serialize};

use crate::protection_checks::{ProtectionFinding, blocks};
use crate::protection_delta::{ProtectionDelta, ProtectionSummary, summarise};

/// What happened to one test, projected for a reviewer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TestLineageView {
    /// Test identity.
    pub test: String,
    /// Lineage token (`unchanged`, `renamed`, `removed`, …).
    pub state: String,
    /// What the two revisions were matched on.
    pub matched_on: String,
    /// Whether the protection it provides changed.
    pub protection_changed: bool,
    /// Flows it no longer reaches.
    pub lost_flows: Vec<String>,
    /// Flows it newly reaches.
    pub gained_flows: Vec<String>,
    /// Present and green, but no longer guarding what it is believed to guard.
    pub phantom: bool,
}

/// One flow before and after, per spec §87 `quality_flow`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FlowView {
    /// Flow identity.
    pub flow: String,
    /// Nodes on base.
    pub base_path: Vec<String>,
    /// Nodes on head.
    pub head_path: Vec<String>,
    /// Requirements the flow serves.
    pub requirements: Vec<String>,
    /// Tests before.
    pub tests_before: Vec<String>,
    /// Tests after.
    pub tests_after: Vec<String>,
    /// Branches measured before.
    pub coverage_before: Vec<String>,
    /// Branches measured after.
    pub coverage_after: Vec<String>,
    /// Proof ids before.
    pub proof_before: Vec<String>,
    /// Proof ids after.
    pub proof_after: Vec<String>,
}

/// Exact human-review identity for one changed sealed expectation.
///
/// The digest belongs to the revision-bound proposal artifact. A generic QA
/// decision about the change or the new seal is deliberately insufficient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleReplacementReview {
    /// One concrete decision subject derived from the proposal digest.
    pub subject: String,
    /// CAS digest of the proposal the reviewer must have seen.
    pub artifact_digest: String,
    /// `OpenSpec` change.
    pub change: String,
    /// Exact base commit measured.
    pub base_revision: String,
    /// Exact head commit measured.
    pub head_revision: String,
    /// Exact Weavatrix content revision executed on head/worktree.
    pub head_content_revision: String,
    /// Common ancestor used for comparison.
    pub merge_base: String,
    /// Seal that governed base.
    pub base_seal: String,
    /// Full digest of the base seal document.
    pub base_seal_digest: String,
    /// Proposed seal on head.
    pub head_seal: String,
    /// Full digest of the proposed head seal document.
    pub head_seal_digest: String,
    /// Obligation ids whose sealed meaning changed.
    pub changed_obligations: Vec<String>,
    /// Explicit base → head obligation replacements.
    pub obligation_replacements: Vec<(String, String)>,
    /// True only after an exact digest-matching QA or product-owner acceptance.
    pub approved: bool,
    /// Provenance of the acceptance.
    pub approval_decision: Option<String>,
}

/// Everything the protection surfaces serve.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProtectionView {
    /// Per-flow deltas.
    pub deltas: Vec<ProtectionDelta>,
    /// Findings from the twelve checks.
    pub findings: Vec<ProtectionFinding>,
    /// Test lineage projections.
    pub lineage: Vec<TestLineageView>,
    /// Full before/after detail per flow.
    pub flows: Vec<FlowView>,
    /// Human review required when the sealed expectation changed.
    pub oracle_replacement: Option<OracleReplacementReview>,
}

/// `quality_protection` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProtectionReport {
    /// Counts by state.
    pub summary: ProtectionSummary,
    /// Only the flows a human needs to look at.
    pub needs_attention: Vec<ProtectionDelta>,
    /// How many healthy flows were counted but not listed.
    pub suppressed_healthy: usize,
    /// Findings from the twelve checks.
    pub findings: Vec<ProtectionFinding>,
    /// Whether anything must fail CI.
    pub blocking: bool,
    /// Exact proposal to review, when the `OracleSeal` changed.
    pub oracle_replacement: Option<OracleReplacementReview>,
}

impl ProtectionView {
    /// Exception-first protection report.
    ///
    /// Preserved and improved flows are counted, never listed: the dashboard is
    /// for what changed for the worse.
    #[must_use]
    pub fn report(&self) -> ProtectionReport {
        let (attention, healthy): (Vec<ProtectionDelta>, Vec<ProtectionDelta>) = self
            .deltas
            .iter()
            .cloned()
            .partition(|item| item.state.is_regression());
        ProtectionReport {
            summary: summarise(&self.deltas),
            needs_attention: attention,
            suppressed_healthy: healthy.len(),
            findings: self.findings.clone(),
            blocking: blocks(&self.findings),
            oracle_replacement: self.oracle_replacement.clone(),
        }
    }

    /// Lineage for one test.
    #[must_use]
    pub fn lineage_of(&self, test: &str) -> Option<&TestLineageView> {
        self.lineage.iter().find(|item| item.test == test)
    }

    /// Before/after detail for one flow.
    #[must_use]
    pub fn flow(&self, flow: &str) -> Option<&FlowView> {
        self.flows.iter().find(|item| item.flow == flow)
    }
}
