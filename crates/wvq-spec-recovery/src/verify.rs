//! Spec §66.4 deterministic checks that run before a human ever looks.
//!
//! Everything here is model-less. The point is to stop QA wasting review time on
//! candidates that are duplicated, self-contradictory, untestable, or written in
//! terms of the implementation they are supposed to judge.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::evidence::{Confidence, IntentEvidence, assess};

/// Wording that promises nothing measurable.
const VAGUE: &[&str] = &[
    "correctly",
    "properly",
    "as expected",
    "look good",
    "looks good",
    "user-friendly",
    "reasonable",
    "reasonably",
    "fast",
    "quickly",
    "slow",
    "appropriate",
    "appropriately",
    "nicely",
    "gracefully",
];

/// Markers that a sentence talks about the implementation, not the behaviour.
const LEAKAGE: &[&str] = &[
    "src/",
    "()",
    ".ts",
    ".tsx",
    ".js",
    ".go",
    ".rs",
    "css",
    "xpath",
    "div",
    "classname",
    "data-testid",
    "#",
    "querySelector",
];

/// What a deterministic check objected to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    /// Another active requirement already says this.
    DuplicateRequirement,
    /// Two candidates assert opposite things about one subject.
    InternalContradiction,
    /// The candidate depends on a surface the head revision removed.
    CodeContradiction,
    /// Observed runtime behaviour disagrees with the proposed intent.
    BehaviorContradiction,
    /// No measurable oracle; the sentence cannot fail.
    NonTestableWording,
    /// The expectation is phrased in implementation terms.
    ImplementationLeakage,
    /// No actor or role.
    MissingActor,
    /// No precondition.
    MissingPrecondition,
    /// No trigger or action.
    MissingTrigger,
    /// A numeric limit without below/at/above cases.
    MissingBoundaryCase,
    /// A permission or async surface without a denial or failure case.
    MissingNegativeCase,
    /// Only the changed implementation and its own test support this.
    WeakOracleIndependence,
}

impl FindingKind {
    /// Stable token for transport.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DuplicateRequirement => "duplicate_requirement",
            Self::InternalContradiction => "internal_contradiction",
            Self::CodeContradiction => "code_contradiction",
            Self::BehaviorContradiction => "behavior_contradiction",
            Self::NonTestableWording => "non_testable_wording",
            Self::ImplementationLeakage => "implementation_leakage",
            Self::MissingActor => "missing_actor",
            Self::MissingPrecondition => "missing_precondition",
            Self::MissingTrigger => "missing_trigger",
            Self::MissingBoundaryCase => "missing_boundary_case",
            Self::MissingNegativeCase => "missing_negative_case",
            Self::WeakOracleIndependence => "weak_oracle_independence",
        }
    }

    /// Whether this must be resolved before the candidate is worth reviewing.
    ///
    /// A weak oracle is shown prominently but does not block review: deciding it
    /// is exactly the human's job.
    #[must_use]
    pub fn blocks_review(self) -> bool {
        self != Self::WeakOracleIndependence
    }
}

/// One objection, bound to the candidate that caused it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerifierFinding {
    /// What was wrong.
    pub kind: FindingKind,
    /// Candidate identity.
    pub candidate: String,
    /// Human-readable explanation with the offending fragment.
    pub detail: String,
}

/// Which extra cases a candidate's shape demands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CandidateShape {
    /// Requirement mentions a numeric limit, so boundaries matter.
    pub numeric_limit: bool,
    /// Requirement touches permissions, so a denial case matters.
    pub permission_sensitive: bool,
    /// Requirement covers async UI, so a failure case matters.
    pub async_ui: bool,
}

/// A recovered requirement waiting to be checked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandidateRequirement {
    /// Candidate identity.
    pub id: String,
    /// Normalised subject, so contradictions can be spotted.
    pub subject: String,
    /// The proposed normative sentence.
    pub text: String,
    /// Whether the sentence asserts the behaviour holds.
    pub expected_to_hold: bool,
    /// Actor or role.
    pub actor: Option<String>,
    /// Precondition.
    pub precondition: Option<String>,
    /// Trigger or action.
    pub trigger: Option<String>,
    /// Endpoint the requirement depends on.
    pub endpoint: Option<String>,
    /// Evidence backing this candidate.
    pub evidence: Vec<IntentEvidence>,
    /// Which extra cases the shape demands.
    pub shape: CandidateShape,
    /// Case labels the candidate already covers.
    pub covered_cases: Vec<String>,
}

/// One observed runtime fact about a subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedFact {
    /// Subject the observation is about.
    pub subject: String,
    /// Whether the behaviour was observed to hold.
    pub holds: bool,
}

/// Everything outside the candidate set that the checks need.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerifyContext {
    /// Active requirement sentences already in `OpenSpec`.
    pub existing_requirements: Vec<String>,
    /// Endpoints the head revision removed.
    pub removed_endpoints: Vec<String>,
    /// Runtime observations from base/head replay.
    pub observed: Vec<ObservedFact>,
}

/// Result of the deterministic pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerifierReport {
    /// Everything the checks objected to, sorted.
    pub findings: Vec<VerifierFinding>,
    /// Per-candidate confidence, so QA sees a weak oracle immediately.
    pub confidence: BTreeMap<String, Confidence>,
}

impl VerifierReport {
    /// Whether anything must be fixed before QA review is worth doing.
    #[must_use]
    pub fn blocks_review(&self) -> bool {
        self.findings.iter().any(|item| item.kind.blocks_review())
    }

    /// Findings of one kind.
    #[must_use]
    pub fn of_kind(&self, kind: FindingKind) -> Vec<&VerifierFinding> {
        self.findings
            .iter()
            .filter(|item| item.kind == kind)
            .collect()
    }

    /// Whether a given candidate has a finding of one kind.
    #[must_use]
    pub fn has(&self, candidate: &str, kind: FindingKind) -> bool {
        self.findings
            .iter()
            .any(|item| item.candidate == candidate && item.kind == kind)
    }
}

/// Run every deterministic check. Output order is stable.
#[must_use]
pub fn verify_candidates(
    candidates: &[CandidateRequirement],
    context: &VerifyContext,
) -> VerifierReport {
    let mut findings = Vec::new();
    let mut confidence = BTreeMap::new();

    for candidate in candidates {
        confidence.insert(candidate.id.clone(), assess(&candidate.evidence));
        check_shape(candidate, &mut findings);
        check_wording(candidate, &mut findings);
        check_duplicate(candidate, context, &mut findings);
        check_code(candidate, context, &mut findings);
        check_behavior(candidate, context, &mut findings);
        check_oracle(candidate, &mut findings);
    }
    check_internal_contradiction(candidates, &mut findings);

    findings.sort_by(|left, right| {
        left.candidate
            .cmp(&right.candidate)
            .then_with(|| left.kind.cmp(&right.kind))
    });
    VerifierReport {
        findings,
        confidence,
    }
}

fn push(findings: &mut Vec<VerifierFinding>, kind: FindingKind, id: &str, detail: String) {
    findings.push(VerifierFinding {
        kind,
        candidate: id.to_owned(),
        detail,
    });
}

fn check_shape(candidate: &CandidateRequirement, findings: &mut Vec<VerifierFinding>) {
    if candidate.actor.is_none() {
        push(
            findings,
            FindingKind::MissingActor,
            &candidate.id,
            "no actor or role is named".into(),
        );
    }
    if candidate.precondition.is_none() {
        push(
            findings,
            FindingKind::MissingPrecondition,
            &candidate.id,
            "no precondition is stated".into(),
        );
    }
    if candidate.trigger.is_none() {
        push(
            findings,
            FindingKind::MissingTrigger,
            &candidate.id,
            "no trigger or action is stated".into(),
        );
    }
    let covered = |label: &str| {
        candidate
            .covered_cases
            .iter()
            .any(|item| item.eq_ignore_ascii_case(label))
    };
    if candidate.shape.numeric_limit {
        let missing: Vec<&str> = ["below", "at", "above"]
            .into_iter()
            .filter(|label| !covered(label))
            .collect();
        if !missing.is_empty() {
            push(
                findings,
                FindingKind::MissingBoundaryCase,
                &candidate.id,
                format!("numeric limit without {} case(s)", missing.join(", ")),
            );
        }
    }
    if candidate.shape.permission_sensitive && !covered("denied") {
        push(
            findings,
            FindingKind::MissingNegativeCase,
            &candidate.id,
            "permission surface without a denial case".into(),
        );
    }
    if candidate.shape.async_ui && !covered("failure") {
        push(
            findings,
            FindingKind::MissingNegativeCase,
            &candidate.id,
            "async surface without a failure case".into(),
        );
    }
}

fn check_wording(candidate: &CandidateRequirement, findings: &mut Vec<VerifierFinding>) {
    let lower = candidate.text.to_ascii_lowercase();
    if let Some(word) = VAGUE.iter().find(|word| lower.contains(*word))
        && !has_measurable_oracle(&lower)
    {
        push(
            findings,
            FindingKind::NonTestableWording,
            &candidate.id,
            format!("`{word}` has no measurable oracle"),
        );
    }
    if let Some(marker) = LEAKAGE.iter().find(|marker| lower.contains(*marker)) {
        push(
            findings,
            FindingKind::ImplementationLeakage,
            &candidate.id,
            format!("`{marker}` names an implementation detail, not observable behaviour"),
        );
    }
}

/// A number or an explicit comparison turns a vague word into something testable.
fn has_measurable_oracle(lower: &str) -> bool {
    lower.chars().any(|ch| ch.is_ascii_digit())
        || ["within", "at most", "at least", "no more than", "equal to"]
            .iter()
            .any(|token| lower.contains(token))
}

fn check_duplicate(
    candidate: &CandidateRequirement,
    context: &VerifyContext,
    findings: &mut Vec<VerifierFinding>,
) {
    let normalised = normalise(&candidate.text);
    if context
        .existing_requirements
        .iter()
        .any(|item| normalise(item) == normalised)
    {
        push(
            findings,
            FindingKind::DuplicateRequirement,
            &candidate.id,
            "an active requirement already says this".into(),
        );
    }
}

fn check_code(
    candidate: &CandidateRequirement,
    context: &VerifyContext,
    findings: &mut Vec<VerifierFinding>,
) {
    let Some(endpoint) = &candidate.endpoint else {
        return;
    };
    if candidate.expected_to_hold && context.removed_endpoints.contains(endpoint) {
        push(
            findings,
            FindingKind::CodeContradiction,
            &candidate.id,
            format!("`{endpoint}` was removed on head but the candidate depends on it"),
        );
    }
}

fn check_behavior(
    candidate: &CandidateRequirement,
    context: &VerifyContext,
    findings: &mut Vec<VerifierFinding>,
) {
    if let Some(fact) = context
        .observed
        .iter()
        .find(|item| item.subject == candidate.subject)
        && fact.holds != candidate.expected_to_hold
    {
        push(
            findings,
            FindingKind::BehaviorContradiction,
            &candidate.id,
            format!(
                "runtime observed `{}` = {}, candidate proposes {}",
                candidate.subject, fact.holds, candidate.expected_to_hold
            ),
        );
    }
}

fn check_oracle(candidate: &CandidateRequirement, findings: &mut Vec<VerifierFinding>) {
    if assess(&candidate.evidence).oracle_independence_is_weak() {
        push(
            findings,
            FindingKind::WeakOracleIndependence,
            &candidate.id,
            "supported only by the changed implementation and its own test".into(),
        );
    }
}

fn check_internal_contradiction(
    candidates: &[CandidateRequirement],
    findings: &mut Vec<VerifierFinding>,
) {
    for (index, candidate) in candidates.iter().enumerate() {
        for other in candidates.iter().skip(index + 1) {
            if candidate.subject == other.subject
                && candidate.expected_to_hold != other.expected_to_hold
            {
                let detail = format!(
                    "`{}` is asserted both ways with {}",
                    candidate.subject, other.id
                );
                push(
                    findings,
                    FindingKind::InternalContradiction,
                    &candidate.id,
                    detail,
                );
                push(
                    findings,
                    FindingKind::InternalContradiction,
                    &other.id,
                    format!(
                        "`{}` is asserted both ways with {}",
                        other.subject, candidate.id
                    ),
                );
            }
        }
    }
}

/// Lowercase, collapse whitespace, drop trailing punctuation.
fn normalise(text: &str) -> String {
    text.to_ascii_lowercase()
        .split_whitespace()
        .map(|word| word.trim_matches(|ch: char| ch == '.' || ch == ',' || ch == ';'))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
