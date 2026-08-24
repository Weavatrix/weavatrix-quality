//! Task 20: Delta Triangle is evidence; unexpected cells become findings.

use wvq_domain::{FindingState, ObligationId, RequirementId, ScenarioId, Severity};
use wvq_proof::{
    CodeDelta, SpecDelta, TriangleReading, classify_triangle, join_triangle, scoped_spec_delta,
};
use wvq_runtime::{Observation, StructuredView, behavior_delta};
use wvq_spec::{ObligationKind, RiskLevel, SpecChangeScope, TestObligation};

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

fn obligation(id: &str, requirement: &str, scenario: &str) -> TestObligation {
    TestObligation {
        id: ObligationId::new(id).unwrap(),
        requirement: RequirementId::new(requirement).unwrap(),
        scenario: ScenarioId::new(scenario).unwrap(),
        kind: ObligationKind::Behavioral,
        condition: None,
        expected: None,
        required_evidence: Vec::new(),
        risk: RiskLevel::High,
    }
}

#[test]
fn one_changed_obligation_cannot_authorize_a_mixed_program() {
    let obligations = vec![
        obligation("export-csv", "product.export-report", "csv-export"),
        obligation(
            "viewer-delete",
            "product.viewer-permissions",
            "delete-denied",
        ),
    ];
    let scope = SpecChangeScope::from_parts(
        Vec::new(),
        vec![("product.export-report".into(), "csv-export".into())],
    );

    let delta = scoped_spec_delta(
        &scope,
        &obligations,
        &[
            ObligationId::new("export-csv").unwrap(),
            ObligationId::new("viewer-delete").unwrap(),
        ],
    );

    assert!(!delta.changed);
    assert_eq!(delta.authorized_obligations, ["export-csv"]);
    assert_eq!(delta.unauthorized_obligations, ["viewer-delete"]);
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
        &SpecDelta::change_wide(false),
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
        &SpecDelta::change_wide(true),
        CodeDelta { changed: true },
        &delta_with_route_change(),
        "p",
    );
    assert!(expected.findings.is_empty());
    let refactor = join_triangle(
        &SpecDelta::change_wide(false),
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
        &SpecDelta::change_wide(true),
        CodeDelta { changed: true },
        &stable(),
        "p",
    );
    assert_eq!(triangle.reading, TriangleReading::IncompleteImplementation);
    assert_eq!(triangle.findings[0].check.as_str(), "WVQ-BEHAV-002");
    assert_eq!(triangle.findings[0].severity, Severity::Warn);
}
