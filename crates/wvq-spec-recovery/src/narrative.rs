//! Spec §65.5 deterministic `ChangeNarrative`.
//!
//! The narrative is built without a model and is what an agent receives instead
//! of the repository. Declared intent, weak naming hints and observed
//! implementation stay in separate fields so a commit title can never be read as
//! a normative statement.

use serde::Serialize;

use crate::evidence::{Confidence, EvidenceTier, IntentEvidence, assess};

/// What changed in code, summarised from Weavatrix evidence.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CodeDeltaSummary {
    /// Changed UI components.
    pub components: Vec<String>,
    /// Endpoints the head revision added.
    pub endpoints_added: Vec<String>,
    /// Endpoints the head revision removed.
    pub endpoints_removed: Vec<String>,
    /// Changed symbols.
    pub changed_symbols: Vec<String>,
}

/// What changed in tests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TestsDelta {
    /// Added test files or cases.
    pub added: Vec<String>,
    /// Modified tests.
    pub changed: Vec<String>,
    /// Removed tests. These matter most for protection continuity.
    pub removed: Vec<String>,
}

/// Everything the narrative builder needs. All of it is revision-bound.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NarrativeInput {
    /// Capability cluster this narrative describes.
    pub change_cluster: String,
    /// Base revision.
    pub base_revision: String,
    /// Head revision.
    pub head_revision: String,
    /// Every collected piece of evidence, any tier.
    pub evidence: Vec<IntentEvidence>,
    /// Code delta summary.
    pub code_delta: CodeDeltaSummary,
    /// Tests delta.
    pub tests_delta: TestsDelta,
    /// Observed behaviour differences.
    pub behavior_delta: Vec<String>,
}

/// Compact revision-bound narrative. Spec §65.5.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangeNarrative {
    /// Capability cluster.
    pub change_cluster: String,
    /// Base revision.
    pub base_revision: String,
    /// Head revision.
    pub head_revision: String,
    /// Tier A and B only. Nothing here was inferred from code.
    pub declared_intent: Vec<IntentEvidence>,
    /// Tier D naming hints, kept as hints. Never normative.
    pub commit_hints: Vec<String>,
    /// Tier C source evidence.
    pub code_delta: CodeDeltaSummary,
    /// Tier C test evidence.
    pub tests_delta: TestsDelta,
    /// Tier C runtime evidence.
    pub behavior_delta: Vec<String>,
    /// Four separate confidence axes.
    pub confidence: Confidence,
}

impl ChangeNarrative {
    /// Whether any declared intent was found at all.
    #[must_use]
    pub fn has_declared_intent(&self) -> bool {
        !self.declared_intent.is_empty()
    }
}

/// Build the narrative. Deterministic: no clock, no model, sorted output.
#[must_use]
pub fn narrate(input: NarrativeInput) -> ChangeNarrative {
    let confidence = assess(&input.evidence);
    let mut declared_intent: Vec<IntentEvidence> = input
        .evidence
        .iter()
        .filter(|item| {
            matches!(
                item.tier(),
                EvidenceTier::DeclaredIntent | EvidenceTier::ReviewedCollaboration
            )
        })
        .cloned()
        .collect();
    declared_intent.sort_by(|left, right| {
        right
            .tier()
            .cmp(&left.tier())
            .then_with(|| left.provenance.cmp(&right.provenance))
    });

    let mut commit_hints: Vec<String> = input
        .evidence
        .iter()
        .filter(|item| item.tier() == EvidenceTier::WeakHint)
        .map(|item| item.text.clone())
        .collect();
    commit_hints.sort();
    commit_hints.dedup();

    let mut behavior_delta = input.behavior_delta;
    behavior_delta.sort();
    behavior_delta.dedup();

    ChangeNarrative {
        change_cluster: input.change_cluster,
        base_revision: input.base_revision,
        head_revision: input.head_revision,
        declared_intent,
        commit_hints,
        code_delta: sorted_code_delta(input.code_delta),
        tests_delta: sorted_tests_delta(input.tests_delta),
        behavior_delta,
        confidence,
    }
}

fn sorted_code_delta(mut delta: CodeDeltaSummary) -> CodeDeltaSummary {
    for list in [
        &mut delta.components,
        &mut delta.endpoints_added,
        &mut delta.endpoints_removed,
        &mut delta.changed_symbols,
    ] {
        list.sort();
        list.dedup();
    }
    delta
}

fn sorted_tests_delta(mut delta: TestsDelta) -> TestsDelta {
    for list in [&mut delta.added, &mut delta.changed, &mut delta.removed] {
        list.sort();
        list.dedup();
    }
    delta
}
