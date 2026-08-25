//! Measured-absent cells become ranked producers. Unmeasured is not a gap.

use std::collections::BTreeSet;

use serde_json::json;
use wvq_intelligence::{
    ApplicationSurfaceKind, EvidenceCell, EvidenceColumn, EvidenceProducer, MeasuredColumn,
    SurfaceEvidenceColumns, application_surface_graph, plan_cheapest_evidence,
    surface_evidence_matrix,
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

fn checkout_graph() -> wvq_intelligence::ApplicationSurfaceGraph {
    application_surface_graph(&json!({
        "nodes": [
            { "id": "route:/checkout", "kind": "route", "label": "/checkout" },
            { "id": "symbol:src/app.ts#checkout", "span": {"file": "src/app.ts", "start_line": 1, "end_line": 4} }
        ],
        "edges": [
            { "source": "route:/checkout", "target": "symbol:src/app.ts#checkout" }
        ]
    }))
}

#[test]
fn an_unmeasured_cell_is_not_a_gap() {
    let graph = pay_graph();
    let matrix = surface_evidence_matrix(&graph, &SurfaceEvidenceColumns::default());
    let plan = plan_cheapest_evidence(&matrix);
    assert!(plan.gaps.is_empty());
}

#[test]
fn a_present_neighbour_does_not_hide_a_measured_gap() {
    let graph = pay_graph();
    let pay = "endpoint:POST /pay".to_string();
    let admin = "endpoint:GET /admin".to_string();
    let columns = SurfaceEvidenceColumns {
        protection: Some(MeasuredColumn {
            present: BTreeSet::from([pay]),
            absent: BTreeSet::from([admin]),
        }),
        ..SurfaceEvidenceColumns::default()
    };
    let matrix = surface_evidence_matrix(&graph, &columns);
    let plan = plan_cheapest_evidence(&matrix);
    assert_eq!(plan.gaps.len(), 1);
    let gap = &plan.gaps[0];
    assert_eq!(gap.surface, "endpoint:GET /admin");
    assert_eq!(gap.column, EvidenceColumn::Protection);
    assert_eq!(gap.cheapest, Some(EvidenceProducer::ExistingTestAdaptation));
    assert_eq!(gap.producers[0].cost, 1);
    assert_eq!(
        gap.producers.last().map(|offer| offer.producer),
        Some(EvidenceProducer::AiTestProgram)
    );
    assert_eq!(gap.producers.last().map(|offer| offer.cost), Some(10));
}

#[test]
fn intent_cannot_be_established_by_a_test_producer() {
    let graph = pay_graph();
    let admin = "endpoint:GET /admin".to_string();
    let columns = SurfaceEvidenceColumns {
        intent: Some(MeasuredColumn {
            present: BTreeSet::new(),
            absent: BTreeSet::from([admin]),
        }),
        ..SurfaceEvidenceColumns::default()
    };
    let matrix = surface_evidence_matrix(&graph, &columns);
    let plan = plan_cheapest_evidence(&matrix);
    assert_eq!(plan.gaps.len(), 1);
    assert_eq!(plan.gaps[0].column, EvidenceColumn::Intent);
    assert_eq!(plan.gaps[0].cheapest, None);
    assert!(plan.gaps[0].producers.is_empty());
}

#[test]
fn proof_without_intent_has_no_producer() {
    let graph = pay_graph();
    let admin = "endpoint:GET /admin".to_string();
    let columns = SurfaceEvidenceColumns {
        intent: Some(MeasuredColumn {
            present: BTreeSet::new(),
            absent: BTreeSet::from([admin.clone()]),
        }),
        proof: Some(MeasuredColumn {
            present: BTreeSet::new(),
            absent: BTreeSet::from([admin]),
        }),
        ..SurfaceEvidenceColumns::default()
    };
    let matrix = surface_evidence_matrix(&graph, &columns);
    let plan = plan_cheapest_evidence(&matrix);
    let proof = plan
        .gaps
        .iter()
        .find(|gap| gap.column == EvidenceColumn::Proof)
        .expect("proof gap");
    assert_eq!(proof.cheapest, None);
}

#[test]
fn runtime_on_an_endpoint_starts_at_a_recorded_session() {
    let graph = pay_graph();
    let admin = "endpoint:GET /admin".to_string();
    let columns = SurfaceEvidenceColumns {
        runtime: Some(MeasuredColumn {
            present: BTreeSet::new(),
            absent: BTreeSet::from([admin]),
        }),
        ..SurfaceEvidenceColumns::default()
    };
    let matrix = surface_evidence_matrix(&graph, &columns);
    let plan = plan_cheapest_evidence(&matrix);
    assert_eq!(plan.gaps[0].cheapest, Some(EvidenceProducer::RecordedSession));
    assert_eq!(plan.gaps[0].producers[0].cost, 2);
    assert!(!plan
        .gaps[0]
        .producers
        .iter()
        .any(|offer| offer.producer == EvidenceProducer::ExistingTestAdaptation));
    assert!(!plan
        .gaps[0]
        .producers
        .iter()
        .any(|offer| offer.producer == EvidenceProducer::StorybookFlow));
}

#[test]
fn a_route_ui_gap_can_use_storybook_before_explore_or_ai() {
    let graph = checkout_graph();
    let route = "route:/checkout".to_string();
    let columns = SurfaceEvidenceColumns {
        ui: Some(MeasuredColumn {
            present: BTreeSet::new(),
            absent: BTreeSet::from([route]),
        }),
        ..SurfaceEvidenceColumns::default()
    };
    let matrix = surface_evidence_matrix(&graph, &columns);
    assert_eq!(matrix.surfaces[0].kind, ApplicationSurfaceKind::Route);
    let plan = plan_cheapest_evidence(&matrix);
    let producers: Vec<_> = plan.gaps[0]
        .producers
        .iter()
        .map(|offer| offer.producer)
        .collect();
    assert_eq!(
        producers,
        vec![
            EvidenceProducer::RecordedSession,
            EvidenceProducer::StorybookFlow,
            EvidenceProducer::BrowserExplore,
            EvidenceProducer::AiTestProgram,
        ]
    );
}

#[test]
fn mutation_has_no_generation_producer() {
    let graph = pay_graph();
    let pay = "endpoint:POST /pay".to_string();
    let columns = SurfaceEvidenceColumns {
        mutation: Some(MeasuredColumn {
            present: BTreeSet::new(),
            absent: BTreeSet::from([pay]),
        }),
        ..SurfaceEvidenceColumns::default()
    };
    let matrix = surface_evidence_matrix(&graph, &columns);
    let plan = plan_cheapest_evidence(&matrix);
    assert_eq!(plan.gaps[0].column, EvidenceColumn::Mutation);
    assert_eq!(plan.gaps[0].cheapest, None);
}

#[test]
fn a_truncated_matrix_marks_the_plan_incomplete() {
    let mut graph = pay_graph();
    graph.truncated = true;
    let matrix = surface_evidence_matrix(&graph, &SurfaceEvidenceColumns::default());
    let plan = plan_cheapest_evidence(&matrix);
    assert!(plan.truncated);
    assert!(plan.gaps.is_empty());
}

#[test]
fn present_cells_are_not_planned() {
    let graph = pay_graph();
    let pay = "endpoint:POST /pay".to_string();
    let admin = "endpoint:GET /admin".to_string();
    let columns = SurfaceEvidenceColumns {
        test: Some(MeasuredColumn::closed_world(
            &graph,
            BTreeSet::from([pay, admin]),
        )),
        ..SurfaceEvidenceColumns::default()
    };
    let matrix = surface_evidence_matrix(&graph, &columns);
    assert!(matrix
        .surfaces
        .iter()
        .all(|row| row.test == EvidenceCell::Present));
    assert!(plan_cheapest_evidence(&matrix).gaps.is_empty());
}
