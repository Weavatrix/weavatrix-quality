//! Task 12: LCOV ranges overlay Weavatrix spans; missing ≠ uncovered.

use serde_json::json;
use wvq_intelligence::{CoverageMeasurement, DebtBaseline, gate_coverage, map_coverage_to_nodes};
use wvq_runtime::{CoverageArtifact, FileCoverage, LineRange};

fn coverage(path: &str, covered: &[(u32, u32)], uncovered: &[(u32, u32)]) -> CoverageArtifact {
    CoverageArtifact {
        files: vec![FileCoverage {
            path: path.into(),
            covered: covered
                .iter()
                .map(|&(start, end)| LineRange { start, end })
                .collect(),
            uncovered: uncovered
                .iter()
                .map(|&(start, end)| LineRange { start, end })
                .collect(),
        }],
    }
}

fn node(
    id: &str,
    file: &str,
    start: u32,
    end: u32,
    extra: &serde_json::Value,
) -> serde_json::Value {
    let mut value = json!({
        "id": id,
        "span": { "file": file, "start_line": start, "end_line": end }
    });
    if let Some(object) = extra.as_object()
        && let Some(dest) = value.as_object_mut()
    {
        for (key, item) in object {
            dest.insert(key.clone(), item.clone());
        }
    }
    value
}

#[test]
fn measured_file_nodes_do_not_need_a_symbol_span() {
    let report = coverage("src/add.js", &[(1, 2)], &[]);
    let mapped = map_coverage_to_nodes(
        Some(&report),
        &json!({"nodes": [{"id": "file:src/add.js", "kind": "file", "label": "src/add.js"}]}),
    )
    .unwrap();
    assert_eq!(mapped.len(), 1);
    assert_eq!(mapped[0].measurement, CoverageMeasurement::Covered);
    assert_eq!(mapped[0].covered_lines, 2);
}

#[test]
fn lcov_ranges_map_onto_graph_node_spans() {
    let cov = coverage("src/add.js", &[(1, 2)], &[(3, 4)]);
    let graph = json!({
        "nodes": [
            node("fn:add", "src/add.js", 1, 2, &json!({})),
            node("fn:overflow", "src/add.js", 3, 4, &json!({}))
        ]
    });
    let mapped = map_coverage_to_nodes(Some(&cov), &graph).unwrap();
    assert_eq!(mapped[0].measurement, CoverageMeasurement::Covered);
    assert_eq!(mapped[0].covered_lines, 2);
    assert_eq!(mapped[1].measurement, CoverageMeasurement::Uncovered);
    assert_eq!(mapped[1].instrumented_lines, 2);
    assert_eq!(mapped[1].covered_lines, 0);
}

#[test]
fn current_weavatrix_nested_spans_map_at_symbol_granularity() {
    let cov = coverage("service/permission.go", &[(3, 3)], &[(5, 5)]);
    let graph = json!({
        "nodes": [
            {
                "id": "symbol:service/permission.go#function:ViewerLabel@3:1",
                "span": {
                    "file": "service/permission.go",
                    "start": {"line": 3, "column": 1},
                    "end": {"line": 3, "column": 6}
                }
            },
            {
                "id": "symbol:service/permission.go#function:CanDelete@5:1",
                "span": {
                    "file": "service/permission.go",
                    "start": {"line": 5, "column": 1},
                    "end": {"line": 5, "column": 6}
                }
            }
        ]
    });

    let mapped = map_coverage_to_nodes(Some(&cov), &graph).unwrap();
    assert_eq!(mapped[0].measurement, CoverageMeasurement::Covered);
    assert_eq!(mapped[1].measurement, CoverageMeasurement::Uncovered);
}

#[test]
fn missing_lcov_is_unmeasured_not_uncovered() {
    let graph = json!({
        "nodes": [node("fn:add", "src/add.js", 1, 4, &json!({ "changed": true, "risk": "high" }))]
    });
    let mapped = map_coverage_to_nodes(None, &graph).unwrap();
    assert_eq!(mapped[0].measurement, CoverageMeasurement::Unmeasured);
    let delta = gate_coverage(None, None, &graph, &graph, &DebtBaseline::default()).unwrap();
    assert!(
        !delta
            .new
            .iter()
            .any(|finding| finding.check.as_str() == "WVQ-COV-001"),
        "missing evidence must not be reported as uncovered"
    );
}

#[test]
fn changed_high_risk_uncovered_node_is_cov_001() {
    let cov = coverage("src/hot.js", &[], &[(1, 10)]);
    let graph = json!({
        "nodes": [node(
            "fn:hot",
            "src/hot.js",
            1,
            10,
            &json!({ "changed": true, "risk": "high" })
        )]
    });
    let delta = gate_coverage(None, Some(&cov), &graph, &graph, &DebtBaseline::default()).unwrap();
    assert!(
        delta
            .new
            .iter()
            .any(|finding| finding.check.as_str() == "WVQ-COV-001")
    );
}

#[test]
fn impacted_coverage_regression_quotes_base_and_head() {
    let base = coverage("src/a.js", &[(1, 10)], &[]);
    let head = coverage("src/a.js", &[(1, 5)], &[(6, 10)]);
    let graph = json!({
        "nodes": [node("fn:a", "src/a.js", 1, 10, &json!({ "changed": true }))]
    });
    let delta = gate_coverage(
        Some(&base),
        Some(&head),
        &graph,
        &graph,
        &DebtBaseline::default(),
    )
    .unwrap();
    let finding = delta
        .new
        .iter()
        .find(|item| item.check.as_str() == "WVQ-COV-003")
        .expect("regression");
    assert!(
        finding.summary.contains("10/10 → 5/10"),
        "{}",
        finding.summary
    );
}

#[test]
fn static_reachability_is_not_measured_coverage() {
    let cov = coverage("src/a.js", &[(1, 1)], &[]);
    let graph = json!({
        "nodes": [node("fn:a", "src/a.js", 1, 1, &json!({}))],
        "static_selected": ["test/a.test.ts", "test/orphan.test.ts"],
        "measured_tests": ["test/a.test.ts"]
    });
    let delta = gate_coverage(None, Some(&cov), &graph, &graph, &DebtBaseline::default()).unwrap();
    let finding = delta
        .new
        .iter()
        .find(|item| item.check.as_str() == "WVQ-COV-006")
        .expect("disagreement");
    assert!(
        finding.summary.contains("static is not measured coverage"),
        "{}",
        finding.summary
    );
}
