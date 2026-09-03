//! Measured-absent cells become ranked producers. Unmeasured is not a gap.

use std::collections::BTreeSet;

use serde_json::json;
use wvq_intelligence::{
    ApplicationSurfaceKind, EvidenceCell, EvidenceColumn, EvidenceNeed, EvidenceProducer,
    MeasuredColumn, ProducerInventory, SurfaceEvidenceColumns, application_surface_graph,
    plan_cheapest_evidence, plan_cheapest_evidence_with, surface_evidence_matrix,
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
fn an_unmeasured_cell_is_a_measurement_need() {
    let graph = pay_graph();
    let matrix = surface_evidence_matrix(&graph, &SurfaceEvidenceColumns::default());
    let plan = plan_cheapest_evidence(&matrix);
    assert!(plan.gaps.iter().all(|gap| gap.need == EvidenceNeed::Unmeasured));
    assert!(plan.gaps.iter().any(|gap| gap.column == EvidenceColumn::Runtime));
}

#[test]
fn a_present_neighbour_does_not_hide_a_measured_gap() {
    let graph = pay_graph();
    let pay = "endpoint:POST /pay".to_string();
    let admin = "endpoint:GET /admin".to_string();
    let columns = SurfaceEvidenceColumns {
        coverage: Some(MeasuredColumn {
            present: BTreeSet::from([pay]),
            absent: BTreeSet::from([admin]),
        }),
        ..SurfaceEvidenceColumns::default()
    };
    let matrix = surface_evidence_matrix(&graph, &columns);
    let plan = plan_cheapest_evidence(&matrix);
    let gap = plan
        .gaps
        .iter()
        .find(|gap| {
            gap.surface == "endpoint:GET /admin"
                && gap.column == EvidenceColumn::Coverage
                && gap.need == EvidenceNeed::MeasuredAbsent
        })
        .expect("admin coverage");
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
    let intent = plan
        .gaps
        .iter()
        .find(|gap| gap.column == EvidenceColumn::Intent && gap.need == EvidenceNeed::MeasuredAbsent)
        .expect("intent");
    assert_eq!(intent.cheapest, Some(EvidenceProducer::SpecRecovery));
    assert_eq!(
        intent
            .producers
            .iter()
            .map(|offer| offer.producer)
            .collect::<Vec<_>>(),
        [EvidenceProducer::SpecRecovery, EvidenceProducer::ProductReview]
    );
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
    let runtime = plan
        .gaps
        .iter()
        .find(|gap| gap.column == EvidenceColumn::Runtime)
        .expect("runtime");
    assert_eq!(runtime.cheapest, Some(EvidenceProducer::RecordedSession));
    assert_eq!(runtime.producers[0].cost, 2);
    assert!(!runtime
        .producers
        .iter()
        .any(|offer| offer.producer == EvidenceProducer::ExistingTestAdaptation));
    assert!(!runtime
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
    let ui = plan
        .gaps
        .iter()
        .find(|gap| gap.column == EvidenceColumn::Ui)
        .expect("ui");
    let producers: Vec<_> = ui.producers.iter().map(|offer| offer.producer).collect();
    assert_eq!(
        producers,
        vec![
            EvidenceProducer::RecordedSession,
            EvidenceProducer::StorybookFlow,
            EvidenceProducer::AiTestProgram,
        ]
    );
    assert!(!producers.contains(&EvidenceProducer::BrowserExplore));
}

#[test]
fn mutation_offers_source_mutation() {
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
    let mutation = plan
        .gaps
        .iter()
        .find(|gap| gap.column == EvidenceColumn::Mutation && gap.need == EvidenceNeed::MeasuredAbsent)
        .expect("mutation");
    assert_eq!(mutation.cheapest, Some(EvidenceProducer::SourceMutation));
    assert_eq!(mutation.producers[0].cost, 4);
}

#[test]
fn a_truncated_matrix_marks_the_plan_incomplete() {
    let mut graph = pay_graph();
    graph.truncated = true;
    let matrix = surface_evidence_matrix(&graph, &SurfaceEvidenceColumns::default());
    let plan = plan_cheapest_evidence(&matrix);
    assert!(plan.truncated);
    assert!(plan.gaps.iter().all(|gap| gap.need == EvidenceNeed::Unmeasured));
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
    assert!(!plan_cheapest_evidence(&matrix)
        .gaps
        .iter()
        .any(|gap| gap.column == EvidenceColumn::Test));
}

#[test]
fn browser_explore_is_not_offered_until_the_closed_loop_exists() {
    assert!(!EvidenceProducer::BrowserExplore.available());
}

#[test]
fn missing_tests_are_not_adapted() {
    let graph = pay_graph();
    let admin = "endpoint:GET /admin".to_string();
    let columns = SurfaceEvidenceColumns {
        test: Some(MeasuredColumn {
            present: BTreeSet::new(),
            absent: BTreeSet::from([admin]),
        }),
        ..SurfaceEvidenceColumns::default()
    };
    let matrix = surface_evidence_matrix(&graph, &columns);
    let inventory = ProducerInventory {
        matching_tests: false,
        stories: false,
        ..ProducerInventory::default()
    };
    let plan = plan_cheapest_evidence_with(&matrix, &inventory);
    let test = plan
        .gaps
        .iter()
        .find(|gap| gap.column == EvidenceColumn::Test)
        .expect("test");
    assert!(!test
        .producers
        .iter()
        .any(|offer| offer.producer == EvidenceProducer::ExistingTestAdaptation));
    assert!(!test
        .producers
        .iter()
        .any(|offer| offer.producer == EvidenceProducer::StorybookFlow));
}

#[test]
fn mutation_stays_empty_when_the_ecosystem_is_not_owned() {
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
    let inventory = ProducerInventory {
        mutation_available: false,
        ..ProducerInventory::default()
    };
    let plan = plan_cheapest_evidence_with(&matrix, &inventory);
    let mutation = plan
        .gaps
        .iter()
        .find(|gap| gap.column == EvidenceColumn::Mutation)
        .expect("mutation");
    assert_eq!(mutation.cheapest, None);
}
