//! Application Surface Graph is a Weavatrix projection. Missing coverage ≠ uncovered.

use serde_json::json;
use wvq_intelligence::{
    ApplicationSurfaceKind, CoverageMeasurement, NodeCoverage, SurfaceCoverageState,
    SurfaceEvidenceKind, application_surface_graph, coverage_autopilot,
    production_nodes_for_binding,
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
fn a_test_binding_reaches_production_through_a_graph_edge() {
    let graph = json!({
        "nodes": [
            {
                "id": "symbol:src/widget.test.ts#renders",
                "span": {"file": "src/widget.test.ts", "start_line": 1, "end_line": 1}
            },
            {
                "id": "symbol:src/widget.ts#Widget",
                "span": {"file": "src/widget.ts", "start_line": 1, "end_line": 1}
            }
        ],
        "edges": [{
            "source": "symbol:src/widget.test.ts#renders",
            "target": "symbol:src/widget.ts#Widget"
        }]
    });
    assert_eq!(
        production_nodes_for_binding(&graph, "src/widget.test.ts").nodes,
        ["symbol:src/widget.ts#Widget"]
    );
    let reach = production_nodes_for_binding(&graph, "src/widget.test.ts");
    assert!(!reach.truncated);
    assert_eq!(
        reach.evidence_paths,
        [vec![
            "symbol:src/widget.test.ts#renders".to_string(),
            "symbol:src/widget.ts#Widget".to_string()
        ]]
    );
    assert!(production_nodes_for_binding(&graph, "src/orphan.test.ts").nodes.is_empty());
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

#[test]
fn one_hit_does_not_cover_a_multi_node_surface() {
    let graph = application_surface_graph(&json!({
        "endpoints": [{
            "id": "POST /pay",
            "nodes": [
                {"id": "symbol:src/payment.ts#charge"},
                {"id": "symbol:src/payment.ts#refund"}
            ]
        }]
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
    assert_eq!(autopilot.partial, ["endpoint:POST /pay"]);
    assert!(autopilot.covered.is_empty());
    assert_eq!(
        autopilot.surfaces[0].state,
        SurfaceCoverageState::MeasuredPartial
    );
    assert_eq!(autopilot.surfaces[0].covered_nodes, 1);
    assert_eq!(autopilot.surfaces[0].unmeasured_nodes, 1);
}

#[test]
fn a_truncated_surface_graph_marks_autopilot_incomplete() {
    let endpoints = (0..=512)
        .map(|index| json!({ "id": format!("GET /s{index}") }))
        .collect::<Vec<_>>();
    let graph = application_surface_graph(&json!({ "endpoints": endpoints }));
    assert!(graph.truncated);
    let autopilot = coverage_autopilot(&graph, &[]);
    assert!(autopilot.truncated);
    assert_eq!(autopilot.surfaces.len(), 512);
}

#[test]
fn reverse_edges_do_not_claim_unrelated_production_nodes() {
    let graph = json!({
        "nodes": [
            {
                "id": "symbol:src/widget.test.ts#renders",
                "span": {"file": "src/widget.test.ts", "start_line": 1, "end_line": 1}
            },
            {
                "id": "symbol:src/widget.ts#Widget",
                "span": {"file": "src/widget.ts", "start_line": 1, "end_line": 1}
            },
            {
                "id": "symbol:src/unrelated.ts#Other",
                "span": {"file": "src/unrelated.ts", "start_line": 1, "end_line": 1}
            }
        ],
        "edges": [
            {
                "source": "symbol:src/widget.test.ts#renders",
                "target": "symbol:src/widget.ts#Widget"
            },
            {
                "source": "symbol:src/unrelated.ts#Other",
                "target": "symbol:src/widget.ts#Widget"
            }
        ]
    });
    let reach = production_nodes_for_binding(&graph, "src/widget.test.ts");
    assert_eq!(reach.nodes, ["symbol:src/widget.ts#Widget"]);
    assert!(!reach.truncated);
}

#[test]
fn a_deeper_than_bound_walk_is_truncated_not_silently_complete() {
    let mut nodes = vec![json!({
        "id": "symbol:src/widget.test.ts#renders",
        "span": {"file": "src/widget.test.ts", "start_line": 1, "end_line": 1}
    })];
    let mut edges = Vec::new();
    let mut previous = "symbol:src/widget.test.ts#renders".to_owned();
    for index in 0..5 {
        let id = format!("symbol:src/mod{index}.ts#f");
        nodes.push(json!({
            "id": id,
            "span": {"file": format!("src/mod{index}.ts"), "start_line": 1, "end_line": 1}
        }));
        edges.push(json!({ "source": previous, "target": id }));
        previous = id;
    }
    let graph = json!({ "nodes": nodes, "edges": edges });
    let reach = production_nodes_for_binding(&graph, "src/widget.test.ts");
    assert!(reach.truncated);
    assert!(
        !reach.nodes.iter().any(|id| id == "symbol:src/mod4.ts#f"),
        "depth-4 ceiling must not silently include the fifth hop: {:?}",
        reach.nodes
    );
}
