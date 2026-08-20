//! Deterministic defect hypotheses. Spec §20, extending §10.10.
//!
//! The rest of WVQ answers "who must look at this". This module answers "what
//! exactly should they check". It never claims a defect exists: it turns the
//! *shape* of a change into named, falsifiable probes, so a reviewer spends
//! their time on the cases most likely to be wrong rather than re-reading a diff.
//!
//! Everything here is model-less. A hypothesis is only emitted when the change
//! carries the structural signal that justifies it.

use serde::Serialize;

/// A structural fact about the change, recovered from the diff and the graph.
///
/// Signals are deliberately narrow. A vague signal produces vague questions,
/// which is exactly the noise this module exists to avoid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeSignal {
    /// A predicate's treatment of the absent value changed, e.g. `x !== false`
    /// became `x === true`.
    DefaultSensitivityFlipped {
        /// Symbol or config key the predicate reads.
        subject: String,
        /// The old predicate, verbatim.
        before: String,
        /// The new predicate, verbatim.
        after: String,
    },
    /// A guard now tests membership of a fixed set.
    MembershipGuardAdded {
        /// Value being tested.
        subject: String,
        /// Members that pass.
        members: Vec<String>,
        /// The full domain, when the graph knows it.
        domain: Vec<String>,
    },
    /// A key was retired from a persisted or normalised structure.
    PersistedKeyRetired {
        /// Key that went.
        key: String,
        /// Where it was retired from.
        scope: String,
    },
    /// A derived value now reads a different source key.
    DerivationSourceMoved {
        /// What is being derived.
        derived: String,
        /// Key it used to read.
        from_key: String,
        /// Key it reads now.
        to_key: String,
    },
    /// A numeric comparison or limit changed.
    BoundaryChanged {
        /// Symbol holding the boundary.
        subject: String,
    },
    /// A permission or role predicate changed.
    PermissionPredicateChanged {
        /// Symbol holding the predicate.
        subject: String,
    },
    /// A collection is folded, grouped or truncated.
    AggregationIntroduced {
        /// Symbol performing the fold.
        subject: String,
    },
    /// A test changed in the same commit as the code it exercises.
    TestMovedWithImplementation {
        /// Test file.
        test: String,
    },
}

/// How much it would matter if the hypothesis turned out true.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HypothesisWeight {
    /// Worth a glance.
    Low,
    /// Worth a deliberate check.
    Medium,
    /// Would be a user-visible defect. Check before merging.
    High,
}

/// One falsifiable question, with the exact cases that settle it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DefectHypothesis {
    /// Stable id (`WVQ-HYP-001`).
    pub id: &'static str,
    /// What to check, phrased so it can be answered yes or no.
    pub question: String,
    /// The structural fact that produced this question.
    pub because: String,
    /// Concrete inputs or states that settle it.
    pub probes: Vec<String>,
    /// How much it would matter.
    pub weight: HypothesisWeight,
}

fn hypothesis(
    id: &'static str,
    weight: HypothesisWeight,
    question: String,
    because: String,
    probes: Vec<String>,
) -> DefectHypothesis {
    DefectHypothesis {
        id,
        question,
        because,
        probes,
        weight,
    }
}

/// Turn structural signals into checkable hypotheses.
///
/// Output is sorted by weight (heaviest first) then id, so a reviewer reads the
/// most consequential question first. An empty signal set yields nothing: a
/// change with no recognised shape must not generate busywork.
#[must_use]
pub fn hypothesise(signals: &[ChangeSignal]) -> Vec<DefectHypothesis> {
    let mut out: Vec<DefectHypothesis> = signals.iter().flat_map(one).collect();
    out.sort_by(|left, right| {
        right
            .weight
            .cmp(&left.weight)
            .then_with(|| left.id.cmp(right.id))
            .then_with(|| left.question.cmp(&right.question))
    });
    out.dedup_by(|left, right| left.id == right.id && left.question == right.question);
    out
}

fn one(signal: &ChangeSignal) -> Vec<DefectHypothesis> {
    match signal {
        ChangeSignal::MembershipGuardAdded {
            subject,
            members,
            domain,
        } => vec![membership_hypothesis(subject, members, domain)],
        other => simple(other),
    }
}

/// The membership guard needs the domain difference, so it is built apart.
fn membership_hypothesis(subject: &str, members: &[String], domain: &[String]) -> DefectHypothesis {
    let mut probes: Vec<String> = domain
        .iter()
        .filter(|item| !members.contains(item))
        .map(|item| format!("`{subject}` = {item} — is the guard's answer right?"))
        .collect();
    probes.push(format!(
        "`{subject}` absent — `has(undefined)` is false, is that intended?"
    ));
    probes.push(format!(
        "a value added to the domain later falls outside {{{}}} by default",
        members.join(", ")
    ));
    hypothesis(
        "WVQ-HYP-002",
        HypothesisWeight::High,
        format!(
            "Is every value of `{subject}` outside {{{}}} genuinely meant to take the other branch?",
            members.join(", ")
        ),
        format!("a membership guard on `{subject}` was introduced"),
        probes,
    )
}

fn simple(signal: &ChangeSignal) -> Vec<DefectHypothesis> {
    match signal {
        ChangeSignal::MembershipGuardAdded { .. } => Vec::new(),
        ChangeSignal::DefaultSensitivityFlipped {
            subject,
            before,
            after,
        } => vec![hypothesis(
            "WVQ-HYP-001",
            HypothesisWeight::High,
            format!(
                "When `{subject}` is absent, does the new predicate reach the opposite branch?"
            ),
            format!("the predicate changed from `{before}` to `{after}`"),
            vec![
                format!("`{subject}` undefined — old branch vs new branch"),
                format!("`{subject}` explicitly false"),
                format!("`{subject}` explicitly true"),
                format!("what is the declared default of `{subject}`, and do all readers agree?"),
            ],
        )],

        ChangeSignal::PersistedKeyRetired { key, scope } => vec![hypothesis(
            "WVQ-HYP-003",
            HypothesisWeight::High,
            format!("What happens to records already stored with `{key}`?"),
            format!("`{key}` was retired from {scope}"),
            vec![
                format!("load an existing record that still carries `{key}`"),
                format!("does anything still read `{key}`, or is it now silently ignored?"),
                format!("does the behaviour that `{key}` used to control change for that record?"),
                "is a migration needed, or is the change intended to be visible?".into(),
            ],
        )],

        ChangeSignal::DerivationSourceMoved {
            derived,
            from_key,
            to_key,
        } => vec![hypothesis(
            "WVQ-HYP-004",
            HypothesisWeight::High,
            format!(
                "On existing data, do `{from_key}` and `{to_key}` ever disagree about `{derived}`?"
            ),
            format!("`{derived}` now derives from `{to_key}` instead of `{from_key}`"),
            vec![
                format!("a record where `{from_key}` and `{to_key}` imply different `{derived}`"),
                format!("a record carrying `{from_key}` but no `{to_key}`"),
                format!("a record carrying neither — what is `{derived}` then?"),
            ],
        )],

        ChangeSignal::BoundaryChanged { subject } => vec![hypothesis(
            "WVQ-HYP-005",
            HypothesisWeight::Medium,
            format!("Is `{subject}` correct at the boundary itself, not just either side?"),
            format!("a comparison on `{subject}` changed"),
            vec![
                format!("`{subject}` one below the limit"),
                format!("`{subject}` exactly at the limit"),
                format!("`{subject}` one above the limit"),
                format!("`{subject}` at zero and at its maximum"),
            ],
        )],

        ChangeSignal::PermissionPredicateChanged { subject } => vec![hypothesis(
            "WVQ-HYP-006",
            HypothesisWeight::High,
            format!("Does `{subject}` still deny everyone it denied before?"),
            format!("a permission predicate on `{subject}` changed"),
            vec![
                "each role that was previously denied — still denied?".into(),
                "tenant mismatch".into(),
                "expired or missing credentials".into(),
                "the deny path is the one that must keep dynamic coverage".into(),
            ],
        )],

        ChangeSignal::AggregationIntroduced { subject } => vec![hypothesis(
            "WVQ-HYP-007",
            HypothesisWeight::Medium,
            format!("Does `{subject}` preserve the total it folds?"),
            format!("`{subject}` folds or truncates a collection"),
            vec![
                "empty collection".into(),
                "one element".into(),
                "exactly at the fold threshold".into(),
                "one past the threshold".into(),
                "is the aggregate additive? a non-additive fold cannot be recovered".into(),
            ],
        )],

        ChangeSignal::TestMovedWithImplementation { test } => vec![hypothesis(
            "WVQ-HYP-008",
            HypothesisWeight::Medium,
            format!("Does `{test}` assert intended behaviour, or the behaviour it now sees?"),
            format!("`{test}` changed in the same commit as the code it exercises"),
            vec![
                "compare the assertion before and after the change".into(),
                "would the old assertion still pass? if not, what decided it was wrong?".into(),
            ],
        )],
    }
}

/// How sure we are the signal really occurred, as opposed to how much it would
/// matter. Spec §59 Stage C: a category is promoted only once its precision is
/// measured on the repository.
///
/// A shadow run over 60 accepted, bug-free commits showed textual detectors
/// firing on a third to a half of them. Consequence and detection quality are
/// therefore tracked apart, and an unmeasured detector never blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalConfidence {
    /// Pattern-matched from text. Precision unmeasured; advisory only.
    Inferred,
    /// Backed by graph or schema evidence that names the exact symbol.
    Confirmed,
}

/// Hypotheses that should stop a change before it merges.
///
/// Both conditions must hold: the consequence must be `High`, and the signal
/// must be `Confirmed`. A high-stakes question raised by a detector whose
/// precision nobody has measured is advice, not a gate.
#[must_use]
pub fn blocking_questions(
    hypotheses: &[DefectHypothesis],
    confidence: SignalConfidence,
) -> Vec<&DefectHypothesis> {
    if confidence != SignalConfidence::Confirmed {
        return Vec::new();
    }
    hypotheses
        .iter()
        .filter(|item| item.weight == HypothesisWeight::High)
        .collect()
}
