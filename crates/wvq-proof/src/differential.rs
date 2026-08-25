//! Delta Triangle: Spec × Code × Behavior is evidence, not a quality percentage.

use std::collections::BTreeSet;

use wvq_domain::{CheckId, FindingState, ObligationId, QualityFinding, Severity, SubjectRef};
use wvq_runtime::BehaviorDelta;
use wvq_spec::{OpenSpecChange, SpecChangeScope, TestObligation};

use crate::code_surface::surface_from_flows;
use crate::protection::FlowProtection;

/// Whether `OpenSpec` intent changed on this change folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecDelta {
    /// Every obligation asserted by this program is authorized by the exact
    /// requirement/scenario scope that changed.
    pub changed: bool,
    /// Program obligations whose own requirement/scenario changed.
    pub authorized_obligations: Vec<String>,
    /// Program obligations outside the changed intent scope.
    pub unauthorized_obligations: Vec<String>,
}

impl SpecDelta {
    /// Compatibility constructor for a change-wide/non-program test.
    #[must_use]
    pub fn change_wide(changed: bool) -> Self {
        Self {
            changed,
            authorized_obligations: Vec::new(),
            unauthorized_obligations: Vec::new(),
        }
    }
}

/// Whether Weavatrix-reported code changed *for this program*.
///
/// A repository-wide `graph_diff` is not enough: the code axis is the
/// intersection of the program's obligations, the flows that proved them, and
/// the Weavatrix nodes that actually changed. Missing mapping is `unmeasured`,
/// never a borrowed global `true`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeDelta {
    /// Measured nonempty intersection of protected nodes and changed nodes.
    pub changed: bool,
    /// False when the intersection cannot be computed.
    pub measured: bool,
    /// Nodes in the intersection, sorted.
    pub intersecting_nodes: Vec<String>,
    /// Why the intersection was not measured.
    pub unmeasured_reason: Option<String>,
}

impl CodeDelta {
    /// Compatibility constructor for tests that already know the code axis.
    #[must_use]
    pub fn change_wide(changed: bool) -> Self {
        Self {
            changed,
            measured: true,
            intersecting_nodes: Vec::new(),
            unmeasured_reason: None,
        }
    }

    /// Honest gap: the program has a code surface but no measurable mapping.
    #[must_use]
    pub fn unmeasured(reason: impl Into<String>) -> Self {
        Self {
            changed: false,
            measured: false,
            intersecting_nodes: Vec::new(),
            unmeasured_reason: Some(reason.into()),
        }
    }
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

impl TriangleReading {
    /// Stable transport token.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExpectedChangeCandidate => "expected_change_candidate",
            Self::UnintendedBehaviorDrift => "unintended_behavior_drift",
            Self::IncompleteImplementation => "incomplete_implementation",
            Self::RequirementWithoutImplementation => "requirement_without_implementation",
            Self::ProbableInternalRefactor => "probable_internal_refactor",
            Self::EnvironmentNondeterminism => "environment_nondeterminism",
            Self::ConfigOrStaleCodeEvidence => "config_or_stale_code_evidence",
            Self::NoChange => "no_change",
        }
    }
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
    /// Whether both sides had a visual digest and structured axes matched.
    pub visual_compared: bool,
    /// Unexpected cells become findings. Expected change / refactor do not.
    pub findings: Vec<QualityFinding>,
}

/// Detect a spec delta from an `OpenSpec` change.
#[must_use]
pub fn spec_delta(change: &OpenSpecChange) -> SpecDelta {
    SpecDelta::change_wide(
        change
            .capabilities
            .iter()
            .any(|capability| !capability.operations.is_empty()),
    )
}

/// Authorize one program only when every obligation it asserts belongs to the
/// exact requirement/scenario scope changed between base and head.
#[must_use]
pub fn scoped_spec_delta(
    scope: &SpecChangeScope,
    obligations: &[TestObligation],
    program_obligations: &[ObligationId],
) -> SpecDelta {
    let by_id = obligations
        .iter()
        .map(|obligation| (obligation.id.as_str(), obligation))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut authorized = Vec::new();
    let mut unauthorized = Vec::new();
    for id in program_obligations {
        let is_authorized = by_id.get(id.as_str()).is_some_and(|obligation| {
            scope.authorizes(
                obligation.requirement.as_str(),
                obligation.scenario.as_str(),
            )
        });
        if is_authorized {
            authorized.push(id.to_string());
        } else {
            unauthorized.push(id.to_string());
        }
    }
    authorized.sort();
    authorized.dedup();
    unauthorized.sort();
    unauthorized.dedup();
    SpecDelta {
        changed: !program_obligations.is_empty() && unauthorized.is_empty(),
        authorized_obligations: authorized,
        unauthorized_obligations: unauthorized,
    }
}

/// Code axis for one program: obligation → implementation surface → changed nodes.
///
/// Test/spec nodes are not production evidence. No implementation mapping is
/// unmeasured. An empty intersection is a measured `false`.
#[must_use]
pub fn scoped_code_delta(
    program_obligations: &[ObligationId],
    flows: &[FlowProtection],
    changed_nodes: &BTreeSet<String>,
) -> CodeDelta {
    if program_obligations.is_empty() {
        return CodeDelta::unmeasured("program asserts no obligation");
    }
    let mut protected = BTreeSet::new();
    let mut mapped = false;
    for obligation in program_obligations {
        let surface = surface_from_flows(obligation.as_str(), flows);
        if surface.has_implementation_mapping() {
            mapped = true;
            protected.extend(surface.implementation_nodes);
        }
    }
    if !mapped {
        return CodeDelta::unmeasured(
            "no protected flow maps these obligations to Weavatrix nodes",
        );
    }
    let intersecting: Vec<String> = protected.intersection(changed_nodes).cloned().collect();
    CodeDelta {
        changed: !intersecting.is_empty(),
        measured: true,
        intersecting_nodes: intersecting,
        unmeasured_reason: None,
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
///
/// Missing code mapping is an attribution gap, not a missing replay. Spec ×
/// Behavior still decides whether a runtime change is authorized.
#[must_use]
pub fn join_triangle(
    spec: &SpecDelta,
    code: &CodeDelta,
    behavior: &BehaviorDelta,
    program_id: &str,
) -> DeltaTriangle {
    let behavior_changed = behavior.changed();
    let code_changed = code.measured && code.changed;
    let reading = if code.measured {
        classify_triangle(spec.changed, code_changed, behavior_changed)
    } else {
        match (spec.changed, behavior_changed) {
            (false, false) => TriangleReading::NoChange,
            (false, true) => TriangleReading::UnintendedBehaviorDrift,
            (true, false) => TriangleReading::RequirementWithoutImplementation,
            (true, true) => TriangleReading::ExpectedChangeCandidate,
        }
    };
    DeltaTriangle {
        axes: TriangleAxes {
            spec: spec.changed,
            code: code_changed,
            behavior: behavior_changed,
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
        visual_compared: behavior.visual_compared,
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
