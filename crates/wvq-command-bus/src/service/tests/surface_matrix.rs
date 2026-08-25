//! Surface Evidence Matrix persist: measured gaps are absent, missing producers are not.

use super::*;
use super::super::verify_debt::verify_from_token;
use wvq_intelligence::EvidenceCell;

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
fn intent_and_protection_are_measured_while_mutation_stays_unmeasured() {
    let matrix = surface_evidence_document(&pay_graph(), &[coverage_record()], &[pay_binding()]).unwrap();
    let pay = matrix
        .surfaces
        .iter()
        .find(|row| row.surface == "endpoint:POST /pay")
        .expect("pay");
    let idle = matrix
        .surfaces
        .iter()
        .find(|row| row.surface == "endpoint:GET /idle")
        .expect("idle");
    assert_eq!(pay.intent, EvidenceCell::Present);
    assert_eq!(pay.test, EvidenceCell::Present);
    assert_eq!(pay.protection, EvidenceCell::Present);
    assert_eq!(idle.intent, EvidenceCell::Absent);
    assert_eq!(idle.protection, EvidenceCell::Absent);
    assert_eq!(pay.mutation, EvidenceCell::Unmeasured);
    assert_eq!(pay.ui, EvidenceCell::Unmeasured);
    assert_eq!(pay.runtime, EvidenceCell::Unmeasured);
}

#[test]
fn persisting_the_matrix_does_not_invent_a_clean_empty_table() {
    let root = TempDir::new("surface-matrix");
    let store = Store::open(&root.0).unwrap();
    let run = RunId::new("run-surface-matrix").unwrap();
    let revision = RevisionId::new("rev-matrix").unwrap();
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
    let absent = load_surface_evidence_matrix(&store, &run).unwrap();
    assert!(!absent.present);

    let mut handles = Vec::new();
    persist_surface_evidence_matrix(
        &store,
        &run,
        &revision,
        &pay_graph(),
        &[coverage_record()],
        &[pay_binding()],
        &mut handles,
    )
    .unwrap();
    let view = load_surface_evidence_matrix(&store, &run).unwrap();
    assert!(view.present);
    assert!(
        view.surfaces
            .iter()
            .any(|row| row.surface == "endpoint:POST /pay" && row.intent == EvidenceCell::Present)
    );
    let reply = verify_from_token("surface-change", "PROVEN");
    assert!(!reply.blocking);
    assert!(!reply.surface_evidence.present);
}

#[test]
fn unknown_matrix_schema_fails_closed() {
    let root = TempDir::new("matrix-schema");
    let store = Store::open(&root.0).unwrap();
    let run = RunId::new("run-matrix-schema").unwrap();
    store
        .put_run(&wvq_store::StoredRun {
            id: run.clone(),
            change_id: "surface".into(),
            revision: RevisionId::new("rev-matrix-schema").unwrap(),
            status: "complete".into(),
            passed: true,
            outcome: "passed".into(),
        })
        .unwrap();
    let mut handles = Vec::new();
    put_json_run_artifact(
        &store,
        &run,
        "artifact-run-matrix-schema-surface-evidence-matrix",
        SURFACE_EVIDENCE_MATRIX_KIND,
        &json!({ "schema_v": 99, "revision": "rev", "truncated": false, "surfaces": [] }),
        &mut handles,
    )
    .unwrap();
    let err = load_surface_evidence_matrix(&store, &run).unwrap_err();
    assert!(
        err.to_string().contains("unknown surface-evidence-matrix schema"),
        "{err}"
    );
}
