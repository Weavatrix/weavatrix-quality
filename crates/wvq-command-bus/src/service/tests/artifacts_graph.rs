//! Graph-diff and coverage artifacts become revision-bound protection flows.

use super::super::delta::graph::graph_diff_changed_nodes;
use super::*;
use wvq_proof::scoped_code_delta;

#[test]
fn graph_diff_collects_changed_node_ids_not_counts() {
    let diff = json!({
        "counts": {
            "nodes_added": 1,
            "nodes_removed": 1,
            "nodes_changed": 1,
            "edges_added": 1,
            "edges_removed": 1
        },
        "nodes": {
            "added": [{"id": "symbol:src/app.ts#statusLabel"}],
            "removed": [{"id": "symbol:src/app.ts#oldLabel"}],
            "changed": [{
                "before": {"id": "symbol:src/theme.ts#oldPalette"},
                "after": {"id": "symbol:src/theme.ts#palette"}
            }]
        },
        "edges": {
            "added": [{"source": "symbol:src/app.ts#statusLabel", "target": "symbol:caller"}],
            "removed": [{"source": "symbol:src/app.ts#oldLabel", "target": "symbol:dead"}]
        }
    });
    let nodes = graph_diff_changed_nodes(&diff).unwrap();
    assert_eq!(
        nodes,
        BTreeSet::from([
            "symbol:src/app.ts#statusLabel".into(),
            "symbol:src/app.ts#oldLabel".into(),
            "symbol:src/theme.ts#oldPalette".into(),
            "symbol:src/theme.ts#palette".into(),
            "symbol:caller".into(),
            "symbol:dead".into(),
        ])
    );
}

#[test]
fn declared_bindings_map_only_the_owned_source_file() {
    let graph = json!({
        "nodes": [
            {
                "id": "symbol:src/app.ts#statusLabel",
                "span": {"file": "src/app.ts", "start_line": 1, "end_line": 1}
            },
            {
                "id": "symbol:src/theme.ts#palette",
                "span": {"file": "src/theme.ts", "start_line": 1, "end_line": 1}
            }
        ]
    });
    let bindings = [TestBinding {
        path: "src/app.ts".into(),
        runner: None,
        suite: None,
        case: None,
        obligations: BTreeSet::from(["export-usable".into()]),
        cost: 100,
        flake_penalty: 0,
    }];
    let flows = declared_code_flows("rev-head", &bindings, &graph);
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].flow, "symbol:src/app.ts#statusLabel");
    assert_eq!(flows[0].proven_obligations, ["export-usable"]);
    assert_eq!(flows[0].covered_nodes, ["symbol:src/app.ts#statusLabel"]);

    let checkout = [wvq_domain::ObligationId::new("export-usable").unwrap()];
    let changed = graph_diff_changed_nodes(&json!({
        "counts": {
            "nodes_added": 1,
            "nodes_removed": 0,
            "nodes_changed": 0,
            "edges_added": 0,
            "edges_removed": 0
        },
        "nodes": {
            "added": [{"id": "symbol:src/theme.ts#palette"}],
            "removed": [],
            "changed": []
        },
        "edges": {"added": [], "removed": []}
    }))
    .unwrap();
    let delta = scoped_code_delta(&checkout, &flows, &changed);
    assert!(delta.measured);
    assert!(!delta.changed);
}

#[test]
fn a_test_file_binding_does_not_create_a_production_code_flow() {
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
        ]
    });
    let bindings = [TestBinding {
        path: "src/widget.test.ts".into(),
        runner: None,
        suite: None,
        case: None,
        obligations: BTreeSet::from(["export-usable".into()]),
        cost: 100,
        flake_penalty: 0,
    }];
    let flows = declared_code_flows("rev-head", &bindings, &graph);
    assert!(
        flows.is_empty(),
        "test nodes must not become production flows: {flows:?}"
    );
}

#[test]
fn live_graph_diff_and_coverage_build_revision_bound_protection() {
    let diff = json!({
        "counts": {
            "nodes_added": 0,
            "nodes_removed": 0,
            "nodes_changed": 1,
            "edges_added": 0,
            "edges_removed": 0
        },
        "nodes": {
            "added": [],
            "removed": [],
            "changed": [{
                "before": {"id": "symbol:old", "span": {"file": "src/lib.rs", "start_line": 1, "end_line": 2}},
                "after": {"id": "symbol:add", "span": {"file": "src/lib.rs", "start_line": 1, "end_line": 2}}
            }]
        },
        "edges": {"added": [], "removed": []}
    });
    let impact =
        live_impacted_surface(&diff, &json!({"impacted_nodes": [{"id": "symbol:caller"}]}))
            .unwrap();
    assert!(impact.base_only.contains(&"symbol:old".into()));
    assert!(impact.head_only.contains(&"symbol:add".into()));
    assert!(impact.head_only.contains(&"symbol:caller".into()));

    let coverage = CoverageArtifact {
        files: vec![wvq_runtime::FileCoverage {
            path: "src/lib.rs".into(),
            covered: vec![wvq_runtime::LineRange { start: 1, end: 2 }],
            uncovered: Vec::new(),
        }],
    };
    let mut record = record("cargo-test");
    record.artifacts.push(ProducedArtifact {
        kind: "coverage".into(),
        path: "coverage#normalized".into(),
        bytes: serde_json::to_vec(&coverage).unwrap(),
    });
    let revision = RevisionId::new("revision-1").unwrap();
    let graph = json!({"nodes": [diff["nodes"]["changed"][0]["after"].clone()]});
    let protection = live_protection_snapshot(Path::new("."), &revision, &graph, &[record], &[])
        .unwrap()
        .unwrap();
    let flow = protection.flow("symbol:add").unwrap();
    assert_eq!(flow.revision, "revision-1");
    assert_eq!(flow.covered_nodes, ["symbol:add"]);
    assert_eq!(flow.tests, ["executor:cargo-test@."]);
}

#[test]
fn a_single_normalized_case_owns_its_coverage_and_obligation() {
    let root = TempDir::new("exact-protection-case");
    let coverage = CoverageArtifact {
        files: vec![wvq_runtime::FileCoverage {
            path: "service/permission.go".into(),
            covered: vec![wvq_runtime::LineRange { start: 3, end: 5 }],
            uncovered: Vec::new(),
        }],
    };
    let mut record = record("go-test");
    record.cwd = "service".into();
    record.artifacts.push(ProducedArtifact {
        kind: "normalized-test-run".into(),
        path: "go-test#normalized".into(),
        bytes: serde_json::to_vec(&NormalizedTestRun {
            cases: vec![wvq_runtime::TestCaseResult {
                name: "TestViewerCannotDelete".into(),
                suite: "fixture.local/product/service".into(),
                status: TestStatus::Pass,
                duration_ms: Some(1),
                message: None,
            }],
            coverage: None,
            raw_artifacts: Vec::new(),
        })
        .unwrap(),
    });
    record.artifacts.push(ProducedArtifact {
        kind: "coverage".into(),
        path: "go-cover.out#normalized".into(),
        bytes: serde_json::to_vec(&coverage).unwrap(),
    });
    let binding = TestBinding {
        path: "service/permission_test.go".into(),
        runner: Some("go-test".into()),
        suite: Some("fixture.local/product/service".into()),
        case: Some("TestViewerCannotDelete".into()),
        obligations: BTreeSet::from(["viewer-deny".into()]),
        cost: 10,
        flake_penalty: 0,
    };
    let revision = RevisionId::new("revision-exact-protector").unwrap();
    let graph = json!({"nodes": [{
        "id": "function:service/permission.go:CanDelete",
        "span": {"file": "service/permission.go", "start_line": 3, "end_line": 5}
    }]});

    let protection = live_protection_snapshot(&root.0, &revision, &graph, &[record], &[binding])
        .unwrap()
        .unwrap();
    let flow = protection
        .flow("function:service/permission.go:CanDelete")
        .unwrap();
    assert_eq!(
        flow.tests,
        ["service/permission_test.go#TestViewerCannotDelete"]
    );
    assert_eq!(
        protection.executed_tests,
        ["service/permission_test.go#TestViewerCannotDelete"]
    );
    assert_eq!(flow.proven_obligations, ["viewer-deny"]);
}
