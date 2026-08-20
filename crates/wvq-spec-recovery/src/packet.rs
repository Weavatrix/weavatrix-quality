//! Spec §65.6 `RecoveryPacket`, §66.3 adaptive questions, and patch preview.
//!
//! The packet is what an agent receives instead of the repository. The questions
//! are what QA is asked instead of "review this". The patch preview is always
//! labelled a proposal: nothing here seals anything.

use std::fmt::Write as _;

use serde::Serialize;

use crate::cluster::CapabilityCluster;
use crate::narrative::{ChangeNarrative, CodeDeltaSummary, TestsDelta};
use crate::verify::{CandidateRequirement, FindingKind, VerifierReport};

/// Endpoints and routes that appeared or disappeared between revisions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PublicSurfaceDelta {
    /// Surfaces the head revision added.
    pub added: Vec<String>,
    /// Surfaces the head revision removed. These drive removal questions.
    pub removed: Vec<String>,
}

/// What a changed test appears to assert. An intent clue, never truth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TestIntentSummary {
    /// Test identity.
    pub test: String,
    /// What the assertion appears to expect.
    pub appears_to_expect: String,
    /// Whether this test changed in the same change as the implementation.
    pub changed_with_implementation: bool,
}

/// Spec §65.6 packet. Bounded, revision-bound, model-free to build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecoveryPacket {
    /// Base revision.
    pub base_revision: String,
    /// Head revision.
    pub head_revision: String,
    /// Deterministic narrative the packet was built from.
    pub narrative: ChangeNarrative,
    /// Capability clusters, with commit provenance.
    pub capability_clusters: Vec<CapabilityCluster>,
    /// Added and removed public surfaces.
    pub public_surface_delta: PublicSurfaceDelta,
    /// Code delta summary.
    pub code_delta_summary: CodeDeltaSummary,
    /// Tests delta.
    pub tests_delta: TestsDelta,
    /// What the changed tests appear to assert.
    pub changed_test_intent: Vec<TestIntentSummary>,
    /// Neighbouring active requirements, for duplicate and contradiction checks.
    pub neighboring_requirements: Vec<String>,
    /// Deterministic quality heuristics that apply to this shape.
    pub quality_heuristics: Vec<String>,
}

/// Build the packet from an already-computed narrative and clusters.
#[must_use]
pub fn packet(
    narrative: ChangeNarrative,
    capability_clusters: Vec<CapabilityCluster>,
    public_surface_delta: PublicSurfaceDelta,
    changed_test_intent: Vec<TestIntentSummary>,
    neighboring_requirements: Vec<String>,
) -> RecoveryPacket {
    let mut heuristics = Vec::new();
    if !public_surface_delta.removed.is_empty() {
        heuristics.push("removed surface: check for residual routes, handlers and tests".into());
    }
    if changed_test_intent
        .iter()
        .any(|item| item.changed_with_implementation)
    {
        heuristics.push("test changed with implementation: compare base and head oracle".into());
    }
    if !narrative.tests_delta.removed.is_empty() {
        heuristics.push("removed test: confirm the behaviour it protected is gone too".into());
    }
    heuristics.sort();

    RecoveryPacket {
        base_revision: narrative.base_revision.clone(),
        head_revision: narrative.head_revision.clone(),
        code_delta_summary: narrative.code_delta.clone(),
        tests_delta: narrative.tests_delta.clone(),
        narrative,
        capability_clusters,
        public_surface_delta,
        changed_test_intent,
        neighboring_requirements,
        quality_heuristics: heuristics,
    }
}

/// Questions WVQ asks instead of "please review this".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Questions {
    /// Answerable by a quality analyst.
    pub for_qa: Vec<String>,
    /// Needs a product decision.
    pub for_product: Vec<String>,
}

impl Questions {
    /// Whether anything must be escalated beyond QA.
    #[must_use]
    pub fn needs_product(&self) -> bool {
        !self.for_product.is_empty()
    }
}

/// Spec §66.3 adaptive checklist. Only relevant questions are asked.
#[must_use]
pub fn questions(candidates: &[CandidateRequirement], report: &VerifierReport) -> Questions {
    let mut for_qa = Vec::new();
    let mut for_product = Vec::new();

    for candidate in candidates {
        let id = &candidate.id;
        if report.has(id, FindingKind::MissingActor) {
            for_qa.push(format!("{id}: which actor or role does this apply to?"));
        }
        if report.has(id, FindingKind::MissingPrecondition) {
            for_qa.push(format!("{id}: what must be true before the trigger?"));
        }
        if report.has(id, FindingKind::MissingTrigger) {
            for_qa.push(format!("{id}: what action produces this result?"));
        }
        if report.has(id, FindingKind::NonTestableWording) {
            for_qa.push(format!("{id}: what measurable result would prove this?"));
        }
        if report.has(id, FindingKind::ImplementationLeakage) {
            for_qa.push(format!(
                "{id}: restate the expectation in observable terms, without naming code."
            ));
        }
        if candidate.shape.numeric_limit {
            for_qa.push(format!(
                "{id}: what happens below, exactly at, and above the limit? \
                 Are there min and max values?"
            ));
        }
        if candidate.shape.permission_sensitive {
            for_qa.push(format!(
                "{id}: what should Admin, Operator and Viewer see, and what about a tenant mismatch?"
            ));
        }
        if candidate.shape.async_ui {
            for_qa.push(format!(
                "{id}: what should happen while loading, on refresh, on a slow response, \
                 on failure, and on a double action?"
            ));
        }
        if report.has(id, FindingKind::DuplicateRequirement) {
            for_qa.push(format!(
                "{id}: an active requirement already says this. Duplicate, or a real difference?"
            ));
        }

        // Escalations. These are product decisions, not QA judgement calls.
        if report.has(id, FindingKind::BehaviorContradiction) {
            for_product.push(format!(
                "{id}: the running system disagrees with the proposed intent. \
                 Which one is correct?"
            ));
        }
        if report.has(id, FindingKind::CodeContradiction) {
            for_product.push(format!(
                "{id}: this depends on a surface the change removed. Was the removal intended?"
            ));
        }
        if report.has(id, FindingKind::InternalContradiction) {
            for_product.push(format!(
                "{id}: two recovered candidates assert opposite things. Which is intended?"
            ));
        }
        if report.has(id, FindingKind::WeakOracleIndependence) {
            for_product.push(format!(
                "{id}: this expectation exists only because the new code and its own test say so. \
                 Confirm it is intended product behaviour."
            ));
        }
    }

    for_qa.sort();
    for_qa.dedup();
    for_product.sort();
    for_product.dedup();
    Questions {
        for_qa,
        for_product,
    }
}

/// Render the `OpenSpec` patch a reviewer would be offered.
///
/// The output is explicitly a proposal. It carries the evidence and the open
/// questions so nobody can mistake it for an approved requirement.
#[must_use]
pub fn preview_patch(
    change: &str,
    candidates: &[CandidateRequirement],
    questions: &Questions,
) -> String {
    let mut out = String::new();
    out.push_str("# PROPOSED — not approved, not sealed\n\n");
    let _ = writeln!(out, "change: {change}\n");
    for candidate in candidates {
        let _ = writeln!(out, "### Requirement {}\n", candidate.id);
        let _ = writeln!(out, "{}\n", candidate.text);
        if let Some(actor) = &candidate.actor {
            let _ = writeln!(out, "- actor: {actor}");
        }
        if let Some(precondition) = &candidate.precondition {
            let _ = writeln!(out, "- GIVEN {precondition}");
        }
        if let Some(trigger) = &candidate.trigger {
            let _ = writeln!(out, "- WHEN {trigger}");
        }
        out.push_str("- evidence:\n");
        for item in &candidate.evidence {
            let _ = writeln!(
                out,
                "  - [{}] {} ({})",
                item.tier().as_str(),
                item.text,
                item.provenance
            );
        }
        out.push('\n');
    }
    if !questions.for_qa.is_empty() {
        out.push_str("## Open questions for QA\n\n");
        for question in &questions.for_qa {
            let _ = writeln!(out, "- {question}");
        }
        out.push('\n');
    }
    if questions.needs_product() {
        out.push_str("## Escalated to product\n\n");
        for question in &questions.for_product {
            let _ = writeln!(out, "- {question}");
        }
        out.push('\n');
    }
    out
}
