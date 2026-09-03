//! Cheapest-evidence plan persist: measured gaps only, never a gate.

use super::*;
use super::super::verify_debt::verify_from_token;
use wvq_intelligence::{EvidenceColumn, EvidenceNeed, EvidenceProducer};

fn pay_binding() -> TestBinding {
    TestBinding {
        path: "src/payment.ts".into(),
        runner: None,
        suite: None,
        case: None,
        obligations: BTreeSet::from(["charge".into()]),
        cost: 10,
        flake_penalty: 0,
    }
}

fn coverage_record() -> ExecutorRecord {
    let coverage = CoverageArtifact {
        files: vec![
            wvq_runtime::FileCoverage {
                path: "src/payment.ts".into(),
                covered: vec![wvq_runtime::LineRange { start: 1, end: 4 }],
                uncovered: Vec::new(),
            },
            wvq_runtime::FileCoverage {
                path: "src/idle.ts".into(),
                covered: Vec::new(),
                uncovered: vec![wvq_runtime::LineRange { start: 1, end: 2 }],
            },
        ],
    };
    let mut record = record("vitest");
    record.artifacts.push(ProducedArtifact {
        kind: "coverage".into(),
        path: "coverage#normalized".into(),
        bytes: serde_json::to_vec(&coverage).unwrap(),
    });
    record
}

fn pay_graph() -> Value {
    json!({
        "endpoints": [
            { "id": "POST /pay", "handler": "symbol:src/payment.ts#charge" },
            { "id": "GET /idle", "handler": "symbol:src/idle.ts#noop" }
        ],
        "nodes": [
            {
                "id": "symbol:src/payment.ts#charge",
                "span": {"file": "src/payment.ts", "start_line": 1, "end_line": 4}
            },
            {
                "id": "symbol:src/idle.ts#noop",
                "span": {"file": "src/idle.ts", "start_line": 1, "end_line": 2}
            }
        ]
    })
}

#[test]
fn idle_coverage_gap_ranks_existing_test_adaptation_first() {
    let plan = cheapest_evidence_document(&pay_graph(), &[coverage_record()], &[pay_binding()]).unwrap();
    let idle = plan
        .gaps
        .iter()
        .find(|gap| gap.surface == "endpoint:GET /idle" && gap.column == EvidenceColumn::Coverage)
        .expect("idle coverage");
    assert_eq!(idle.cheapest, Some(EvidenceProducer::ExistingTestAdaptation));
    assert!(!plan.gaps.iter().any(|gap| {
        gap.column == EvidenceColumn::Mutation && gap.need == EvidenceNeed::MeasuredAbsent
    }));
    assert!(!plan.gaps.iter().any(|gap| {
        gap.column == EvidenceColumn::Runtime && gap.need == EvidenceNeed::MeasuredAbsent
    }));
    assert!(!plan.gaps.iter().any(|gap| {
        gap.column == EvidenceColumn::Protection && gap.need == EvidenceNeed::MeasuredAbsent
    }));
    assert!(plan.gaps.iter().any(|gap| {
        gap.column == EvidenceColumn::Mutation && gap.need == EvidenceNeed::Unmeasured
    }));
}

#[test]
fn persisting_the_plan_does_not_invent_a_clean_empty_table() {
    let root = TempDir::new("evidence-plan");
    let store = Store::open(&root.0).unwrap();
    let run = RunId::new("run-evidence-plan").unwrap();
    let revision = RevisionId::new("rev-plan").unwrap();
    store
        .put_run(&wvq_store::StoredRun {
            id: run.clone(),
            change_id: "surface".into(),
            revision: revision.clone(),
            status: "complete".into(),
            passed: true,
            outcome: "passed".into(),
        })
        .unwrap();
    let absent = load_cheapest_evidence_plan(&store, &run).unwrap();
    assert!(!absent.present);

    let mut handles = Vec::new();
    persist_cheapest_evidence_plan(
        &store,
        &run,
        &revision,
        &pay_graph(),
        &[coverage_record()],
        &[pay_binding()],
        &mut handles,
    )
    .unwrap();
    let view = load_cheapest_evidence_plan(&store, &run).unwrap();
    assert!(view.present);
    assert!(view.gaps.iter().any(|gap| {
        gap.surface == "endpoint:GET /idle"
            && gap.column == EvidenceColumn::Coverage
            && gap.cheapest == Some(EvidenceProducer::ExistingTestAdaptation)
    }));
    let reply = verify_from_token("surface-change", "PROVEN");
    assert!(!reply.blocking);
    assert!(!reply.evidence_plan.present);
}

#[test]
fn unknown_plan_schema_fails_closed() {
    let root = TempDir::new("plan-schema");
    let store = Store::open(&root.0).unwrap();
    let run = RunId::new("run-plan-schema").unwrap();
    store
        .put_run(&wvq_store::StoredRun {
            id: run.clone(),
            change_id: "surface".into(),
            revision: RevisionId::new("rev-plan-schema").unwrap(),
            status: "complete".into(),
            passed: true,
            outcome: "passed".into(),
        })
        .unwrap();
    let mut handles = Vec::new();
    put_json_run_artifact(
        &store,
        &run,
        "artifact-run-plan-schema-cheapest-evidence-plan",
        CHEAPEST_EVIDENCE_PLAN_KIND,
        &json!({ "schema_v": 99, "revision": "rev", "truncated": false, "gaps": [] }),
        &mut handles,
    )
    .unwrap();
    let err = load_cheapest_evidence_plan(&store, &run).unwrap_err();
    assert!(
        err.to_string().contains("unknown cheapest-evidence-plan schema"),
        "{err}"
    );
}
