//! Surface Evidence Matrix: missing producer ≠ absent. Not a gate.

use std::collections::BTreeSet;

use serde_json::json;
use wvq_intelligence::{
    EvidenceCell, MeasuredColumn, SurfaceEvidenceColumns, application_surface_graph,
    surface_evidence_matrix, surfaces_touching_nodes,
};

fn pay_graph() -> wvq_intelligence::ApplicationSurfaceGraph {
    application_surface_graph(&json!({
        "endpoints": [
            { "id": "POST /pay", "handler": "symbol:src/payment.ts#charge" },
            { "id": "GET /admin", "handler": "symbol:src/admin.ts#delete" }
        ],
        "nodes": [
            { "id": "symbol:src/payment.ts#charge" },
            { "id": "symbol:src/admin.ts#delete" }
        ]
    }))
}

#[test]
fn a_missing_column_is_unmeasured_not_absent() {
    let graph = pay_graph();
    let matrix = surface_evidence_matrix(&graph, &SurfaceEvidenceColumns::default());
    assert_eq!(matrix.surfaces.len(), 2);
    for row in &matrix.surfaces {
        assert_eq!(row.intent, EvidenceCell::Unmeasured);
        assert_eq!(row.runtime, EvidenceCell::Unmeasured);
        assert_eq!(row.test, EvidenceCell::Unmeasured);
        assert_eq!(row.proof, EvidenceCell::Unmeasured);
        assert_eq!(row.coverage, EvidenceCell::Unmeasured);
        assert_eq!(row.protection, EvidenceCell::Unmeasured);
        assert_eq!(row.ui, EvidenceCell::Unmeasured);
        assert_eq!(row.a11y, EvidenceCell::Unmeasured);
        assert_eq!(row.mutation, EvidenceCell::Unmeasured);
    }
}

#[test]
fn a_measured_gap_is_absent_and_does_not_hide_a_present_neighbour() {
    let graph = pay_graph();
    let pay = "endpoint:POST /pay".to_string();
    let admin = "endpoint:GET /admin".to_string();
    let columns = SurfaceEvidenceColumns {
        intent: Some(MeasuredColumn::closed_world(
            &graph,
            BTreeSet::from([pay.clone()]),
        )),
        protection: Some(MeasuredColumn {
            present: BTreeSet::from([pay.clone()]),
            absent: BTreeSet::from([admin.clone()]),
        }),
        mutation: Some(MeasuredColumn {
            present: BTreeSet::new(),
            absent: BTreeSet::from([pay.clone(), admin.clone()]),
        }),
        ..SurfaceEvidenceColumns::default()
    };
    let matrix = surface_evidence_matrix(&graph, &columns);
    let pay_row = matrix
        .surfaces
        .iter()
        .find(|row| row.surface == pay)
        .expect("pay");
    let admin_row = matrix
        .surfaces
        .iter()
        .find(|row| row.surface == admin)
        .expect("admin");
    assert_eq!(pay_row.intent, EvidenceCell::Present);
    assert_eq!(admin_row.intent, EvidenceCell::Absent);
    assert_eq!(pay_row.coverage, EvidenceCell::Unmeasured);
    assert_eq!(pay_row.protection, EvidenceCell::Present);
    assert_eq!(admin_row.protection, EvidenceCell::Absent);
    assert_eq!(pay_row.mutation, EvidenceCell::Absent);
    assert_eq!(admin_row.runtime, EvidenceCell::Unmeasured);
    assert_eq!(admin_row.test, EvidenceCell::Unmeasured);
    assert_eq!(admin_row.proof, EvidenceCell::Unmeasured);
    assert_eq!(admin_row.ui, EvidenceCell::Unmeasured);
    assert_eq!(admin_row.a11y, EvidenceCell::Unmeasured);
}

#[test]
fn a_present_and_absent_conflict_stays_unmeasured() {
    let graph = pay_graph();
    let pay = "endpoint:POST /pay".to_string();
    let columns = SurfaceEvidenceColumns {
        test: Some(MeasuredColumn {
            present: BTreeSet::from([pay.clone()]),
            absent: BTreeSet::from([pay.clone()]),
        }),
        ..SurfaceEvidenceColumns::default()
    };
    let matrix = surface_evidence_matrix(&graph, &columns);
    let pay_row = matrix
        .surfaces
        .iter()
        .find(|row| row.surface == pay)
        .expect("pay");
    assert_eq!(pay_row.test, EvidenceCell::Unmeasured);
}

#[test]
fn implementation_nodes_join_only_the_owned_surface() {
    let graph = pay_graph();
    let touching = surfaces_touching_nodes(&graph, &["symbol:src/payment.ts#charge".to_string()]);
    assert_eq!(touching, BTreeSet::from(["endpoint:POST /pay".into()]));
    assert!(surfaces_touching_nodes(&graph, &[] as &[String; 0]).is_empty());
}

#[test]
fn a_truncated_surface_graph_marks_the_matrix_incomplete() {
    let endpoints = (0..=512)
        .map(|index| json!({ "id": format!("GET /s{index}") }))
        .collect::<Vec<_>>();
    let graph = application_surface_graph(&json!({ "endpoints": endpoints }));
    assert!(graph.truncated);
    let matrix = surface_evidence_matrix(&graph, &SurfaceEvidenceColumns::default());
    assert!(matrix.truncated);
    assert_eq!(matrix.surfaces.len(), 512);
}
