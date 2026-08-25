//! Task 20: Delta Triangle is evidence; unexpected cells become findings.

use std::collections::BTreeSet;

use wvq_domain::{FindingState, ObligationId, RequirementId, ScenarioId, Severity};
use wvq_proof::{
    CodeDelta, FlowProtection, SpecDelta, TriangleReading, classify_triangle, join_triangle,
    scoped_code_delta, scoped_spec_delta,
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

fn flow(flow: &str, obligations: &[&str], nodes: &[&str]) -> FlowProtection {
    FlowProtection {
        flow: flow.into(),
        revision: "rev-head".into(),
        tests: vec!["protector.spec".into()],
        sessions: Vec::new(),
        covered_nodes: nodes.iter().map(|item| (*item).to_string()).collect(),
        covered_branches: Vec::new(),
        proven_obligations: obligations.iter().map(|item| (*item).to_string()).collect(),
        proofs: Vec::new(),
    }
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
fn a_program_that_asserts_no_obligation_is_never_authorized() {
    let scope = SpecChangeScope::from_parts(vec!["product.export-report".into()], Vec::new());
    let delta = scoped_spec_delta(
        &scope,
        &[obligation(
            "export-csv",
            "product.export-report",
            "csv-export",
        )],
        &[],
    );
    assert!(!delta.changed);
    assert!(delta.authorized_obligations.is_empty());
    assert!(delta.unauthorized_obligations.is_empty());
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
        &CodeDelta::change_wide(true),
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
        &CodeDelta::change_wide(true),
        &delta_with_route_change(),
        "p",
    );
    assert!(expected.findings.is_empty());
    let refactor = join_triangle(
        &SpecDelta::change_wide(false),
        &CodeDelta::change_wide(true),
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
        &CodeDelta::change_wide(true),
        &stable(),
        "p",
    );
    assert_eq!(triangle.reading, TriangleReading::IncompleteImplementation);
    assert_eq!(triangle.findings[0].check.as_str(), "WVQ-BEHAV-002");
    assert_eq!(triangle.findings[0].severity, Severity::Warn);
}

#[test]
fn a_theme_node_does_not_satisfy_a_checkout_program() {
    let checkout = [ObligationId::new("export-usable").unwrap()];
    let flows = [flow(
        "symbol:src/app.ts#statusLabel",
        &["export-usable"],
        &["symbol:src/app.ts#statusLabel"],
    )];
    let changed = BTreeSet::from(["symbol:src/theme.ts#palette".into()]);

    let delta = scoped_code_delta(&checkout, &flows, &changed);

    assert!(delta.measured);
    assert!(!delta.changed);
    assert!(delta.intersecting_nodes.is_empty());
    assert!(delta.unmeasured_reason.is_none());
}

#[test]
fn matching_protected_nodes_are_a_measured_code_delta() {
    let checkout = [ObligationId::new("export-usable").unwrap()];
    let flows = [flow(
        "symbol:src/app.ts#statusLabel",
        &["export-usable"],
        &[
            "symbol:src/app.ts#statusLabel",
            "symbol:src/app.ts#exportLabel",
        ],
    )];
    let changed = BTreeSet::from([
        "symbol:src/app.ts#statusLabel".into(),
        "symbol:src/theme.ts#palette".into(),
    ]);

    let delta = scoped_code_delta(&checkout, &flows, &changed);

    assert!(delta.measured);
    assert!(delta.changed);
    assert_eq!(delta.intersecting_nodes, ["symbol:src/app.ts#statusLabel"]);
}

#[test]
fn missing_flow_mapping_is_unmeasured_never_a_borrowed_true() {
    let checkout = [ObligationId::new("export-usable").unwrap()];
    let flows = [flow(
        "symbol:src/theme.ts#palette",
        &["theme-visible"],
        &["symbol:src/theme.ts#palette"],
    )];
    let changed = BTreeSet::from(["symbol:src/theme.ts#palette".into()]);

    let delta = scoped_code_delta(&checkout, &flows, &changed);

    assert!(!delta.measured);
    assert!(!delta.changed);
    assert_eq!(
        delta.unmeasured_reason.as_deref(),
        Some("no protected flow maps these obligations to Weavatrix nodes")
    );
}

#[test]
fn a_program_that_asserts_no_obligation_cannot_claim_a_code_delta() {
    let flows = [flow(
        "symbol:src/app.ts#statusLabel",
        &["export-usable"],
        &["symbol:src/app.ts#statusLabel"],
    )];
    let changed = BTreeSet::from(["symbol:src/app.ts#statusLabel".into()]);

    let delta = scoped_code_delta(&[], &flows, &changed);

    assert!(!delta.measured);
    assert!(!delta.changed);
    assert_eq!(
        delta.unmeasured_reason.as_deref(),
        Some("program asserts no obligation")
    );
}

#[test]
fn unmeasured_code_does_not_emit_behavior_drift_findings() {
    let triangle = join_triangle(
        &SpecDelta::change_wide(false),
        &CodeDelta::unmeasured("no protected flow maps these obligations to Weavatrix nodes"),
        &delta_with_route_change(),
        "checkout-ui",
    );
    assert!(!triangle.axes.code);
    assert!(triangle.axes.behavior);
    assert!(triangle.findings.is_empty());
}
