//! Application Surface Graph is a Weavatrix projection. Missing coverage ≠ uncovered.

use serde_json::json;
use wvq_intelligence::{
    ApplicationSurfaceKind, CoverageMeasurement, NodeCoverage, SurfaceCoverageState,
    SurfaceEvidenceKind, application_surface_graph, coverage_autopilot,
};

#[test]
fn an_endpoint_claims_its_handler_not_the_test_file() {
    let graph = json!({
        "endpoints": [{
            "id": "POST /pay",
            "handler": "symbol:src/payment.ts#charge"
        }],
        "nodes": [
            {
                "id": "symbol:src/payment.ts#charge",
                "span": {"file": "src/payment.ts", "start_line": 1, "end_line": 8}
            },
            {
                "id": "symbol:src/payment.test.ts#charges",
                "span": {"file": "src/payment.test.ts", "start_line": 1, "end_line": 4}
            }
        ],
        "edges": [{
            "source": "POST /pay",
            "target": "symbol:src/payment.ts#charge"
        }]
    });
    let projected = application_surface_graph(&graph);
    let pay = projected
        .surfaces
        .iter()
        .find(|surface| surface.id == "endpoint:POST /pay")
        .expect("pay surface");
    assert_eq!(pay.kind, ApplicationSurfaceKind::Endpoint);
    assert_eq!(pay.evidence, SurfaceEvidenceKind::WeavatrixEndpoint);
    assert_eq!(pay.implementation_nodes, ["symbol:src/payment.ts#charge"]);
    assert!(
        projected
            .surfaces
            .iter()
            .all(|surface| !surface.id.contains("payment.test")),
        "test files must not become production surfaces: {:?}",
        projected.surfaces
    );
}

#[test]
fn a_route_node_reaches_implementation_through_an_edge() {
    let graph = json!({
        "nodes": [
            {"id": "route:/checkout", "kind": "route", "label": "/checkout"},
            {
                "id": "symbol:src/app.ts#statusLabel",
                "span": {"file": "src/app.ts", "start_line": 1, "end_line": 3}
            }
        ],
        "edges": [{
            "source": "route:/checkout",
            "target": "symbol:src/app.ts#statusLabel"
        }]
    });
    let projected = application_surface_graph(&graph);
    assert_eq!(projected.surfaces.len(), 1);
    assert_eq!(projected.surfaces[0].id, "route:/checkout");
    assert_eq!(
        projected.surfaces[0].implementation_nodes,
        ["symbol:src/app.ts#statusLabel"]
    );
}

#[test]
fn missing_lcov_is_unmeasured_not_uncovered() {
    let graph = application_surface_graph(&json!({
        "endpoints": [{ "id": "POST /pay", "handler": "symbol:src/payment.ts#charge" }]
    }));
    let autopilot = coverage_autopilot(&graph, &[]);
    assert_eq!(autopilot.unmeasured, ["endpoint:POST /pay"]);
    assert!(autopilot.uncovered.is_empty());
    assert_eq!(
        autopilot.surfaces[0].state,
        SurfaceCoverageState::Unmeasured
    );
}

#[test]
fn a_hit_implementation_node_covers_the_surface() {
    let graph = application_surface_graph(&json!({
        "endpoints": [{ "id": "POST /pay", "handler": "symbol:src/payment.ts#charge" }]
    }));
    let autopilot = coverage_autopilot(
        &graph,
        &[NodeCoverage {
            node_id: "symbol:src/payment.ts#charge".into(),
            measurement: CoverageMeasurement::Covered,
            covered_lines: 3,
            instrumented_lines: 3,
        }],
    );
    assert_eq!(autopilot.covered, ["endpoint:POST /pay"]);
    assert!(autopilot.unmeasured.is_empty());
    assert_eq!(
        autopilot.surfaces[0].state,
        SurfaceCoverageState::MeasuredCovered
    );
}

#[test]
fn an_instrumented_zero_hit_surface_is_uncovered() {
    let graph = application_surface_graph(&json!({
        "endpoints": [{ "id": "POST /pay", "handler": "symbol:src/payment.ts#charge" }]
    }));
    let autopilot = coverage_autopilot(
        &graph,
        &[NodeCoverage {
            node_id: "symbol:src/payment.ts#charge".into(),
            measurement: CoverageMeasurement::Uncovered,
            covered_lines: 0,
            instrumented_lines: 4,
        }],
    );
    assert_eq!(autopilot.uncovered, ["endpoint:POST /pay"]);
    assert!(autopilot.unmeasured.is_empty());
}
