//! Delta Triangle: Spec × Code × Behavior is evidence, not a quality percentage.

use wvq_domain::{CheckId, FindingState, QualityFinding, Severity, SubjectRef};
use wvq_runtime::BehaviorDelta;
use wvq_spec::OpenSpecChange;

/// Whether `OpenSpec` intent changed on this change folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpecDelta {
    /// Any ADDED / MODIFIED / REMOVED / RENAMED requirement.
    pub changed: bool,
}

/// Whether Weavatrix-reported code changed. WVQ does not own the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeDelta {
    /// Impacted nodes/files on base ∪ head ∪ removed.
    pub changed: bool,
}

/// Spec-§5 reading of the three-axis table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriangleReading {
    /// Spec, code, and behavior all moved.
    ExpectedChangeCandidate,
    /// Code and behavior moved without a spec delta.
    UnintendedBehaviorDrift,
    /// Spec and code moved; behavior did not.
    IncompleteImplementation,
    /// Spec moved; no code or behavior evidence.
    RequirementWithoutImplementation,
    /// Code moved; spec and behavior did not.
    ProbableInternalRefactor,
    /// Behavior moved; spec and code did not.
    EnvironmentNondeterminism,
    /// Spec and behavior moved without code evidence.
    ConfigOrStaleCodeEvidence,
    /// Nothing moved.
    NoChange,
}

/// The three Delta Triangle axes. Evidence, not a verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriangleAxes {
    /// Spec axis: intent changed.
    pub spec: bool,
    /// Code axis: Weavatrix-reported code changed.
    pub code: bool,
    /// Behavior axis: structured runtime changed.
    pub behavior: bool,
}

/// Joined triangle plus unexpected findings. Not a `Proof` verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaTriangle {
    /// Spec / code / behavior bits.
    pub axes: TriangleAxes,
    /// Table reading.
    pub reading: TriangleReading,
    /// First structured behavior axis, when behavior moved.
    pub first_behavior_axis: Option<String>,
    /// Whether pixel handles were compared (only if structured matched).
    pub pixel_compared: bool,
    /// Unexpected cells become findings. Expected change / refactor do not.
    pub findings: Vec<QualityFinding>,
}

/// Detect a spec delta from an `OpenSpec` change.
#[must_use]
pub fn spec_delta(change: &OpenSpecChange) -> SpecDelta {
    SpecDelta {
        changed: change
            .capabilities
            .iter()
            .any(|capability| !capability.operations.is_empty()),
    }
}

/// Classify the three booleans. The table is evidence, never a collapsed %.
#[must_use]
pub fn classify_triangle(spec: bool, code: bool, behavior: bool) -> TriangleReading {
    match (spec, code, behavior) {
        (true, true, true) => TriangleReading::ExpectedChangeCandidate,
        (false, true, true) => TriangleReading::UnintendedBehaviorDrift,
        (true, true, false) => TriangleReading::IncompleteImplementation,
        (true, false, false) => TriangleReading::RequirementWithoutImplementation,
        (false, true, false) => TriangleReading::ProbableInternalRefactor,
        (false, false, true) => TriangleReading::EnvironmentNondeterminism,
        (true, false, true) => TriangleReading::ConfigOrStaleCodeEvidence,
        (false, false, false) => TriangleReading::NoChange,
    }
}

/// Join spec, code, and behavior deltas and emit unexpected findings.
#[must_use]
pub fn join_triangle(
    spec: SpecDelta,
    code: CodeDelta,
    behavior: &BehaviorDelta,
    program_id: &str,
) -> DeltaTriangle {
    let reading = classify_triangle(spec.changed, code.changed, behavior.changed());
    DeltaTriangle {
        axes: TriangleAxes {
            spec: spec.changed,
            code: code.changed,
            behavior: behavior.changed(),
        },
        reading,
        first_behavior_axis: behavior
            .first_structured
            .map(wvq_runtime::DiffAxis::as_str)
            .map(ToOwned::to_owned)
            .or_else(|| {
                behavior
                    .axes
                    .first()
                    .map(|item| item.axis.as_str().to_owned())
            }),
        pixel_compared: behavior.pixel_compared,
        findings: unexpected_findings(reading, program_id),
    }
}

fn unexpected_findings(reading: TriangleReading, program_id: &str) -> Vec<QualityFinding> {
    let (check, severity, summary) = match reading {
        TriangleReading::UnintendedBehaviorDrift => (
            "WVQ-BEHAV-001",
            Severity::Error,
            "runtime behavior changed without a spec delta",
        ),
        TriangleReading::IncompleteImplementation => (
            "WVQ-BEHAV-002",
            Severity::Warn,
            "spec and code changed but behavior did not",
        ),
        TriangleReading::RequirementWithoutImplementation => (
            "WVQ-BEHAV-003",
            Severity::Warn,
            "spec changed with no code or behavior evidence",
        ),
        TriangleReading::EnvironmentNondeterminism => (
            "WVQ-BEHAV-004",
            Severity::Warn,
            "behavior changed with no spec or code delta",
        ),
        TriangleReading::ConfigOrStaleCodeEvidence => (
            "WVQ-BEHAV-005",
            Severity::Warn,
            "spec and behavior changed without code evidence",
        ),
        TriangleReading::ExpectedChangeCandidate
        | TriangleReading::ProbableInternalRefactor
        | TriangleReading::NoChange => return Vec::new(),
    };
    vec![QualityFinding {
        check: CheckId::new(check).expect("static WVQ-BEHAV ids are non-empty"),
        severity,
        state: FindingState::New,
        subject: SubjectRef::Test(program_id.to_owned()),
        summary: summary.to_owned(),
        weavatrix_fingerprint: None,
    }]
}
