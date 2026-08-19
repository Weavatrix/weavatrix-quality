//! Task 20: Delta Triangle is evidence; unexpected cells become findings.

use wvq_domain::{FindingState, Severity};
use wvq_proof::{CodeDelta, SpecDelta, TriangleReading, classify_triangle, join_triangle};
use wvq_runtime::{Observation, StructuredView, behavior_delta};

fn delta_with_route_change() -> wvq_runtime::BehaviorDelta {
    let base = StructuredView::from_replay(
        &Observation {
            route: Some("/sankey".into()),
            ..Observation::default()
        },
        None,
    );
    let head = StructuredView::from_replay(
        &Observation {
            route: Some("/sankey-v2".into()),
            ..Observation::default()
        },
        None,
    );
    behavior_delta(&base, &head)
}

fn stable() -> wvq_runtime::BehaviorDelta {
    let view = StructuredView::from_replay(&Observation::default(), None);
    behavior_delta(&view, &view)
}

#[test]
fn matrix_matches_spec_table() {
    assert_eq!(
        classify_triangle(true, true, true),
        TriangleReading::ExpectedChangeCandidate
    );
    assert_eq!(
        classify_triangle(false, true, true),
        TriangleReading::UnintendedBehaviorDrift
    );
    assert_eq!(
        classify_triangle(true, true, false),
        TriangleReading::IncompleteImplementation
    );
    assert_eq!(
        classify_triangle(true, false, false),
        TriangleReading::RequirementWithoutImplementation
    );
    assert_eq!(
        classify_triangle(false, true, false),
        TriangleReading::ProbableInternalRefactor
    );
    assert_eq!(
        classify_triangle(false, false, true),
        TriangleReading::EnvironmentNondeterminism
    );
    assert_eq!(
        classify_triangle(true, false, true),
        TriangleReading::ConfigOrStaleCodeEvidence
    );
    assert_eq!(
        classify_triangle(false, false, false),
        TriangleReading::NoChange
    );
}

#[test]
fn unexpected_drift_is_an_explicit_finding() {
    let triangle = join_triangle(
        SpecDelta { changed: false },
        CodeDelta { changed: true },
        &delta_with_route_change(),
        "sankey-others-replay",
    );
    assert_eq!(triangle.reading, TriangleReading::UnintendedBehaviorDrift);
    assert_eq!(triangle.findings.len(), 1);
    assert_eq!(triangle.findings[0].check.as_str(), "WVQ-BEHAV-001");
    assert_eq!(triangle.findings[0].severity, Severity::Error);
    assert_eq!(triangle.findings[0].state, FindingState::New);
    assert_eq!(triangle.first_behavior_axis.as_deref(), Some("route"));
    assert!(triangle.axes.behavior);
    assert!(!triangle.axes.spec);
}

#[test]
fn expected_change_and_refactor_are_not_findings() {
    let expected = join_triangle(
        SpecDelta { changed: true },
        CodeDelta { changed: true },
        &delta_with_route_change(),
        "p",
    );
    assert!(expected.findings.is_empty());
    let refactor = join_triangle(
        SpecDelta { changed: false },
        CodeDelta { changed: true },
        &stable(),
        "p",
    );
    assert_eq!(refactor.reading, TriangleReading::ProbableInternalRefactor);
    assert!(refactor.findings.is_empty());
}

#[test]
fn incomplete_implementation_is_a_warning_finding() {
    let triangle = join_triangle(
        SpecDelta { changed: true },
        CodeDelta { changed: true },
        &stable(),
        "p",
    );
    assert_eq!(triangle.reading, TriangleReading::IncompleteImplementation);
    assert_eq!(triangle.findings[0].check.as_str(), "WVQ-BEHAV-002");
    assert_eq!(triangle.findings[0].severity, Severity::Warn);
}
