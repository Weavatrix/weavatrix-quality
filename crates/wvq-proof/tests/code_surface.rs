//! Obligation code surfaces never treat test nodes as production evidence.

use std::collections::BTreeSet;

use wvq_domain::ObligationId;
use wvq_proof::{
    CodeSurfaceEvidenceKind, FlowProtection, is_test_source_path, obligations_owning_path,
    partition_code_nodes, scoped_code_delta, surface_from_flows, surfaces_from_declared_paths,
};

fn flow(flow: &str, obligations: &[&str], nodes: &[&str]) -> FlowProtection {
    FlowProtection {
        flow: flow.into(),
        revision: "rev-head".into(),
        tests: vec!["protector.spec.ts".into()],
        sessions: Vec::new(),
        covered_nodes: nodes.iter().map(|item| (*item).to_string()).collect(),
        covered_branches: Vec::new(),
        proven_obligations: obligations.iter().map(|item| (*item).to_string()).collect(),
        proofs: Vec::new(),
    }
}

#[test]
fn test_source_paths_are_not_implementation() {
    assert!(is_test_source_path("src/widget.test.ts"));
    assert!(is_test_source_path("limit/limit_test.go"));
    assert!(is_test_source_path("src/widget.stories.tsx"));
    assert!(!is_test_source_path("src/widget.ts"));
    assert!(!is_test_source_path("limit/limit.go"));
}

#[test]
fn partition_keeps_test_nodes_out_of_implementation() {
    let (implementation, tests) = partition_code_nodes([
        "symbol:src/app.ts#statusLabel",
        "symbol:src/app.test.ts#renders",
        "file:src/theme.ts",
    ]);
    assert_eq!(
        implementation,
        ["file:src/theme.ts", "symbol:src/app.ts#statusLabel"]
    );
    assert_eq!(tests, ["symbol:src/app.test.ts#renders"]);
}

#[test]
fn coverage_of_a_test_node_is_not_implementation_evidence() {
    let surface = surface_from_flows(
        "export-usable",
        &[flow(
            "symbol:src/app.test.ts#renders",
            &["export-usable"],
            &["symbol:src/app.test.ts#renders"],
        )],
    );
    assert!(!surface.has_implementation_mapping());
    assert_eq!(surface.test_nodes, ["symbol:src/app.test.ts#renders"]);
    assert!(surface.evidence.is_empty());
}

#[test]
fn implementation_coverage_is_exact_dynamic_evidence() {
    let surface = surface_from_flows(
        "export-usable",
        &[flow(
            "symbol:src/app.ts#statusLabel",
            &["export-usable"],
            &[
                "symbol:src/app.ts#statusLabel",
                "symbol:src/app.test.ts#renders",
            ],
        )],
    );
    assert_eq!(
        surface.implementation_nodes,
        ["symbol:src/app.ts#statusLabel"]
    );
    assert_eq!(surface.test_nodes, ["symbol:src/app.test.ts#renders"]);
    assert_eq!(
        surface.evidence[0].kind,
        CodeSurfaceEvidenceKind::ExactDynamicCoverage
    );
}

#[test]
fn a_test_file_binding_does_not_claim_production_code_delta() {
    let surfaces = surfaces_from_declared_paths(&[(
        "src/widget.test.ts".into(),
        BTreeSet::from(["export-usable".into()]),
    )]);
    assert_eq!(surfaces.len(), 1);
    assert!(!surfaces[0].has_implementation_mapping());
    assert_eq!(surfaces[0].test_nodes, ["src/widget.test.ts"]);

    let checkout = [ObligationId::new("export-usable").unwrap()];
    let flows = [flow(
        "symbol:src/widget.test.ts#renders",
        &["export-usable"],
        &["symbol:src/widget.test.ts#renders"],
    )];
    let changed = BTreeSet::from(["symbol:src/widget.test.ts#renders".into()]);
    let delta = scoped_code_delta(&checkout, &flows, &changed);
    assert!(!delta.measured);
    assert!(!delta.changed);
}

#[test]
fn payment_surface_does_not_let_pagination_judge_a_payment_mutant() {
    let surfaces = surfaces_from_declared_paths(&[
        (
            "src/payment.ts".into(),
            BTreeSet::from(["pay-allowed".into()]),
        ),
        (
            "src/pagination.ts".into(),
            BTreeSet::from(["page-bound".into()]),
        ),
    ]);
    let judged = obligations_owning_path(
        &surfaces,
        "src/payment.ts",
        &["pay-allowed".into(), "page-bound".into()],
    );
    assert_eq!(judged, ["pay-allowed"]);
}

#[test]
fn unmapped_mutant_path_is_not_judged_by_unrelated_obligations() {
    let surfaces = surfaces_from_declared_paths(&[(
        "src/widget.test.ts".into(),
        BTreeSet::from(["export-usable".into()]),
    )]);
    let judged = obligations_owning_path(
        &surfaces,
        "src/widget.ts",
        &["export-usable".into()],
    );
    assert!(
        judged.is_empty(),
        "no owner must not fall back to every candidate: {judged:?}"
    );
}
