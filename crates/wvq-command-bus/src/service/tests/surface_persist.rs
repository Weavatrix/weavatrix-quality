//! Application Surface Graph is persisted as a read-only projection, never a gate.

use super::*;
use super::super::verify_debt::verify_from_token;
use crate::ApplicationSurfaceView;
use wvq_store::StoredRun;

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
            },
            { "id": "route:/checkout", "kind": "route", "label": "/checkout" },
            {
                "id": "symbol:src/app.ts#statusLabel",
                "span": {"file": "src/app.ts", "start_line": 1, "end_line": 3}
            },
            {
                "id": "symbol:src/app.ts#theme",
                "span": {"file": "src/app.ts", "start_line": 4, "end_line": 6}
            }
        ],
        "edges": [
            { "source": "route:/checkout", "target": "symbol:src/app.ts#statusLabel" },
            { "source": "route:/checkout", "target": "symbol:src/app.ts#theme" }
        ]
    })
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
                path: "src/app.ts".into(),
                covered: vec![wvq_runtime::LineRange { start: 1, end: 3 }],
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

#[test]
fn mixed_surfaces_project_to_protected_partial_and_unmeasured() {
    let document = application_surface_document(
        &RevisionId::new("rev-surface").unwrap(),
        &pay_graph(),
        &[coverage_record()],
    )
    .unwrap();
    assert_eq!(document.schema_v, 1);
    assert!(!document.truncated);
    assert_eq!(document.protected, ["endpoint:POST /pay"]);
    assert_eq!(document.partial, ["route:/checkout"]);
    assert_eq!(document.unmeasured, ["endpoint:GET /idle"]);
    assert!(!document.unmeasured.contains(&"endpoint:POST /pay".into()));
}

#[test]
fn missing_coverage_is_unmeasured_not_a_clean_empty_graph() {
    let document = application_surface_document(
        &RevisionId::new("rev-empty-cov").unwrap(),
        &pay_graph(),
        &[record("vitest")],
    )
    .unwrap();
    assert!(document.protected.is_empty());
    assert!(document.partial.is_empty());
    assert_eq!(
        document.unmeasured,
        [
            "endpoint:GET /idle".to_string(),
            "endpoint:POST /pay".into(),
            "route:/checkout".into()
        ]
    );
}

#[test]
fn a_missing_artifact_is_absent_not_an_empty_clean_view() {
    let root = TempDir::new("surface-absent");
    let store = Store::open(&root.0).unwrap();
    let run = RunId::new("run-surface-absent").unwrap();
    store
        .put_run(&StoredRun {
            id: run.clone(),
            change_id: "surface".into(),
            revision: RevisionId::new("rev-absent").unwrap(),
            status: "complete".into(),
            passed: true,
            outcome: "passed".into(),
        })
        .unwrap();
    let view = load_application_surface(&store, &run).unwrap();
    assert!(!view.present, "missing evidence is not an empty graph");
    assert!(view.protected.is_empty());
    assert!(view.unmeasured.is_empty());
}

#[test]
fn persisted_surface_graph_explains_a_named_surface() {
    let root = TempDir::new("surface-explain");
    let store = Store::open(&root.0).unwrap();
    let run = RunId::new("run-surface-explain").unwrap();
    let revision = RevisionId::new("rev-explain").unwrap();
    store
        .put_run(&StoredRun {
            id: run.clone(),
            change_id: "surface".into(),
            revision: revision.clone(),
            status: "complete".into(),
            passed: true,
            outcome: "passed".into(),
        })
        .unwrap();
    let mut handles = Vec::new();
    persist_application_surface_graph(
        &store,
        &run,
        &revision,
        &pay_graph(),
        &[coverage_record()],
        &mut handles,
    )
    .unwrap();
    assert!(
        handles.iter().any(|handle| handle.contains("application-surface-graph")),
        "run must name the CAS handle: {handles:?}"
    );

    let view = load_application_surface(&store, &run).unwrap();
    assert!(view.present);
    assert_eq!(view.protected, ["endpoint:POST /pay"]);
    assert_eq!(view.partial, ["route:/checkout"]);
    assert_eq!(view.unmeasured, ["endpoint:GET /idle"]);

    let explained = explain_application_surface(&store, "endpoint:POST /pay")
        .unwrap()
        .expect("protected surface");
    assert_eq!(explained.kind, "application_surface");
    assert!(explained.summary.contains("protected"));
    assert!(
        explained
            .provenance
            .iter()
            .any(|line| line.contains("artifact application-surface-graph")),
        "{:?}",
        explained.provenance
    );
}

#[test]
fn unknown_surface_schema_fails_closed() {
    let root = TempDir::new("surface-schema");
    let store = Store::open(&root.0).unwrap();
    let run = RunId::new("run-surface-schema").unwrap();
    store
        .put_run(&StoredRun {
            id: run.clone(),
            change_id: "surface".into(),
            revision: RevisionId::new("rev-schema").unwrap(),
            status: "complete".into(),
            passed: true,
            outcome: "passed".into(),
        })
        .unwrap();
    let mut handles = Vec::new();
    put_json_run_artifact(
        &store,
        &run,
        "artifact-run-surface-schema-application-surface-graph",
        APPLICATION_SURFACE_GRAPH_KIND,
        &json!({ "schema_v": 99, "protected": [], "partial": [], "unmeasured": [] }),
        &mut handles,
    )
    .unwrap();
    let err = load_application_surface(&store, &run).unwrap_err();
    assert!(
        err.to_string().contains("unknown application-surface-graph schema"),
        "{err}"
    );
}

#[test]
fn an_absent_surface_view_does_not_block_verify() {
    let view = ApplicationSurfaceView::absent();
    assert!(!view.present);
    let reply = verify_from_token("surface-change", "PROVEN");
    assert!(!reply.blocking);
    assert!(!reply.application_surface.present);
    assert!(!reply.surface_evidence.present);
}

#[test]
fn a_recorded_journal_adds_behavior_surfaces_without_crossing_dimensions() {
    let root = TempDir::new("behavior-surface");
    let store = Store::open(&root.0).unwrap();
    let run = RunId::new("run-behavior").unwrap();
    let revision = RevisionId::new("rev-behavior").unwrap();
    store
        .put_run(&StoredRun {
            id: run.clone(),
            change_id: "surface".into(),
            revision: revision.clone(),
            status: "complete".into(),
            passed: true,
            outcome: "passed".into(),
        })
        .unwrap();
    let journal = wvq_runtime::ContinuousJournal::from_json(
        r#"{
            "schema_v": 1,
            "source": "continuous",
            "observed_only": true,
            "session_id": "staging-checkout",
            "initial": { "route": "/checkout" },
            "events": [
                {
                    "action": { "action": "activate", "target": { "test_id": "pay" } },
                    "after": { "route": "/checkout", "actor": "admin" }
                },
                {
                    "action": { "action": "activate", "target": { "test_id": "pay" } },
                    "after": { "route": "/checkout", "data_class": "empty_cart" }
                }
            ]
        }"#,
    )
    .unwrap();
    let mut handles = Vec::new();
    persist_behavior_surface_graph(
        &store,
        &run,
        &revision,
        &pay_graph(),
        std::slice::from_ref(&journal),
        &mut handles,
    )
    .unwrap();
    let value = read_single_run_json(&store, &run, BEHAVIOR_SURFACE_GRAPH_KIND).unwrap();
    let ids = value["behaviors"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert!(
        ids.iter()
            .any(|id| id == "route:/checkout|role:admin|action:activate"),
        "{ids:?}"
    );
    assert!(
        ids.iter()
            .any(|id| id == "route:/checkout|state:empty_cart|action:activate"),
        "{ids:?}"
    );
    assert!(
        ids.iter()
            .all(|id| !(id.contains("role:admin") && id.contains("state:empty_cart"))),
        "admin × empty_cart must not be invented: {ids:?}"
    );
}
