//! Surface Evidence Matrix persist: measured gaps are absent, missing producers are not.

use super::super::verify_debt::verify_from_token;
use super::*;
use crate::source_mutation::{MutationResultRecord, MutationRunDocument};
use wvq_domain::ArtifactId;
use wvq_intelligence::EvidenceCell;
use wvq_proof::{FlowProtection, ProtectionSnapshot};
use wvq_runtime::ContinuousJournal;

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
fn intent_and_coverage_are_measured_while_protection_stays_unmeasured() {
    let matrix =
        surface_evidence_document(&pay_graph(), &[coverage_record()], &[pay_binding()]).unwrap();
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
    assert_eq!(pay.coverage, EvidenceCell::Present);
    assert_eq!(pay.protection, EvidenceCell::Unmeasured);
    assert_eq!(idle.intent, EvidenceCell::Absent);
    assert_eq!(idle.test, EvidenceCell::Absent);
    assert_eq!(idle.coverage, EvidenceCell::Absent);
    assert_eq!(idle.protection, EvidenceCell::Unmeasured);
    assert_eq!(pay.mutation, EvidenceCell::Unmeasured);
    assert_eq!(pay.ui, EvidenceCell::Unmeasured);
    assert_eq!(pay.runtime, EvidenceCell::Unmeasured);
}

#[test]
fn a_binding_without_obligations_is_test_not_intent() {
    let mut binding = pay_binding();
    binding.obligations.clear();
    let matrix = surface_evidence_document(&pay_graph(), &[coverage_record()], &[binding]).unwrap();
    let pay = matrix
        .surfaces
        .iter()
        .find(|row| row.surface == "endpoint:POST /pay")
        .expect("pay");
    assert_eq!(pay.intent, EvidenceCell::Absent);
    assert_eq!(pay.test, EvidenceCell::Present);
    assert_eq!(pay.coverage, EvidenceCell::Present);
    assert_eq!(pay.protection, EvidenceCell::Unmeasured);
}

#[test]
fn live_producers_fill_protection_and_mutation_without_calling_them_coverage() {
    let mutation = MutationRunDocument {
        schema_v: 1,
        state: "measured".into(),
        obligations: vec!["charge".into()],
        applicable_obligations: vec!["charge".into()],
        planned: 1,
        killed: 1,
        survived: 0,
        invalid: 0,
        results: vec![MutationResultRecord {
            id: "m1".into(),
            ecosystem: "ts_js".into(),
            operator: "boundary_flip".into(),
            path: "src/payment.ts".into(),
            line: 2,
            column: 1,
            status: "killed".into(),
            obligation: "charge".into(),
            tests_run: vec!["vitest#charge".into()],
        }],
        limitations: Vec::new(),
        runtime_llm_tokens: 0,
    };
    let protection = ProtectionSnapshot {
        revision: "rev-head".into(),
        executed_tests: vec!["src/payment.ts#charge".into()],
        flows: vec![FlowProtection {
            flow: "symbol:src/payment.ts#charge".into(),
            revision: "rev-head".into(),
            tests: vec!["src/payment.ts".into()],
            sessions: Vec::new(),
            covered_nodes: vec!["symbol:src/payment.ts#charge".into()],
            covered_branches: Vec::new(),
            proven_obligations: vec!["charge".into()],
            proofs: Vec::new(),
        }],
    };
    let graph = pay_graph();
    let record = coverage_record();
    let binding = pay_binding();
    let sources = SurfaceEvidenceSources {
        graph: &graph,
        records: std::slice::from_ref(&record),
        bindings: std::slice::from_ref(&binding),
        mutation: Some(&mutation),
        browser_runs: &[],
        ui: None,
        protection: Some(&protection),
        journals: &[],
    };
    let matrix = surface_evidence_from(&sources).unwrap();
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
    assert_eq!(pay.coverage, EvidenceCell::Present);
    assert_eq!(pay.protection, EvidenceCell::Present);
    assert_eq!(pay.mutation, EvidenceCell::Present);
    assert_eq!(idle.protection, EvidenceCell::Absent);
    assert_eq!(idle.mutation, EvidenceCell::Absent);
    assert_eq!(pay.runtime, EvidenceCell::Unmeasured);
    assert_eq!(pay.ui, EvidenceCell::Unmeasured);
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
    assert!(view
        .surfaces
        .iter()
        .any(|row| row.surface == "endpoint:POST /pay" && row.intent == EvidenceCell::Present));
    let reply = verify_from_token("surface-change", "PROVEN");
    assert!(!reply.blocking);
    assert!(!reply.surface_evidence.present);
}

#[test]
fn a_v1_matrix_treats_old_protection_as_coverage() {
    let root = TempDir::new("matrix-v1");
    let store = Store::open(&root.0).unwrap();
    let run = RunId::new("run-matrix-v1").unwrap();
    store
        .put_run(&wvq_store::StoredRun {
            id: run.clone(),
            change_id: "surface".into(),
            revision: RevisionId::new("rev-matrix-v1").unwrap(),
            status: "complete".into(),
            passed: true,
            outcome: "passed".into(),
        })
        .unwrap();
    let mut handles = Vec::new();
    put_json_run_artifact(
        &store,
        &run,
        "artifact-run-matrix-v1-surface-evidence-matrix",
        SURFACE_EVIDENCE_MATRIX_KIND,
        &json!({
            "schema_v": 1,
            "revision": "rev",
            "truncated": false,
            "surfaces": [{
                "surface": "endpoint:POST /pay",
                "kind": "endpoint",
                "intent": "present",
                "runtime": "unmeasured",
                "test": "present",
                "proof": "unmeasured",
                "protection": "present",
                "ui": "unmeasured",
                "a11y": "unmeasured",
                "mutation": "unmeasured"
            }]
        }),
        &mut handles,
    )
    .unwrap();
    let view = load_surface_evidence_matrix(&store, &run).unwrap();
    let pay = view
        .surfaces
        .iter()
        .find(|row| row.surface == "endpoint:POST /pay")
        .expect("pay");
    assert_eq!(pay.coverage, EvidenceCell::Present);
    assert_eq!(pay.protection, EvidenceCell::Unmeasured);
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
        err.to_string()
            .contains("unknown surface-evidence-matrix schema"),
        "{err}"
    );
}

fn checkout_graph() -> Value {
    json!({
        "endpoints": [
            { "id": "POST /pay", "handler": "symbol:src/payment.ts#charge" }
        ],
        "nodes": [
            { "id": "route:/checkout", "kind": "route", "label": "/checkout" },
            { "id": "route:/idle", "kind": "route", "label": "/idle" },
            {
                "id": "symbol:src/payment.ts#charge",
                "span": {"file": "src/payment.ts", "start_line": 1, "end_line": 4}
            }
        ]
    })
}

fn checkout_journal() -> ContinuousJournal {
    ContinuousJournal::from_json(checkout_journal_raw()).unwrap()
}

#[test]
fn a_continuous_journal_fills_runtime_and_does_not_claim_intent_or_proof() {
    let journal = checkout_journal();
    let graph = checkout_graph();
    let sources = SurfaceEvidenceSources {
        graph: &graph,
        records: &[],
        bindings: &[],
        mutation: None,
        browser_runs: &[],
        ui: None,
        protection: None,
        journals: std::slice::from_ref(&journal),
    };
    let matrix = surface_evidence_from(&sources).unwrap();
    let checkout = matrix
        .surfaces
        .iter()
        .find(|row| row.surface == "route:/checkout")
        .expect("checkout");
    let idle = matrix
        .surfaces
        .iter()
        .find(|row| row.surface == "route:/idle")
        .expect("idle");
    let pay = matrix
        .surfaces
        .iter()
        .find(|row| row.surface == "endpoint:POST /pay")
        .expect("pay");
    assert_eq!(checkout.runtime, EvidenceCell::Present);
    assert_eq!(idle.runtime, EvidenceCell::Absent);
    assert_eq!(
        pay.runtime,
        EvidenceCell::Absent,
        "a journal action is not an API identity and must not invent an endpoint hit"
    );
    assert_ne!(checkout.intent, EvidenceCell::Present);
    assert_ne!(pay.intent, EvidenceCell::Present);
    assert_eq!(checkout.proof, EvidenceCell::Unmeasured);
    assert_eq!(pay.proof, EvidenceCell::Unmeasured);
    assert_eq!(checkout.test, EvidenceCell::Absent);
}

#[test]
fn stored_journals_are_loaded_for_the_runtime_column() {
    let root = TempDir::new("journal-runtime");
    let store = Store::open(&root.0).unwrap();
    let id = ArtifactId::new("artifact-session-staging-checkout-journal").unwrap();
    store
        .put_artifact(
            &id,
            CONTINUOUS_OBSERVATION_JOURNAL_KIND,
            checkout_journal_raw().as_bytes(),
        )
        .unwrap();
    let loaded = load_continuous_journals(&store).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].session_id, "staging-checkout");
}

fn checkout_journal_raw() -> &'static str {
    r#"{
        "schema_v": 1,
        "source": "continuous",
        "observed_only": true,
        "session_id": "staging-checkout",
        "initial": { "route": "/checkout" },
        "events": [
            {
                "action": { "action": "activate", "target": { "test_id": "pay" } },
                "after": { "route": "/checkout" }
            }
        ]
    }"#
}
