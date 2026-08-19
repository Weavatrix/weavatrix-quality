//! Map measured LCOV ranges onto Weavatrix node spans.
//!
//! Missing coverage is never treated as uncovered. Static reachability is
//! never treated as measured coverage.

use serde_json::Value;

use crate::weavatrix::IntelligenceError;
use wvq_domain::{CheckId, QualityFinding, Severity, SubjectRef};
use wvq_runtime::{CoverageArtifact, LineRange};

/// How a node relates to measured coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageMeasurement {
    /// At least one instrumented line in the span was hit.
    Covered,
    /// The span is instrumented and every hit count is zero.
    Uncovered,
    /// No measured report covers this span. Not evidence of absence.
    Unmeasured,
}

/// Measured coverage for one Weavatrix graph node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeCoverage {
    /// Graph node id (Weavatrix, not a second graph).
    pub node_id: String,
    /// Measurement kind.
    pub measurement: CoverageMeasurement,
    /// Hit lines overlapping the node span.
    pub covered_lines: u64,
    /// Instrumented lines overlapping the node span (`DA` entries).
    pub instrumented_lines: u64,
}

/// Overlay LCOV ranges onto Weavatrix node spans.
///
/// # Errors
///
/// Fails closed when a node has no id.
pub fn map_coverage_to_nodes(
    coverage: Option<&CoverageArtifact>,
    graph: &Value,
) -> Result<Vec<NodeCoverage>, IntelligenceError> {
    let mut out = Vec::new();
    for node in graph_nodes(graph) {
        let id = node_id(node).ok_or_else(|| {
            IntelligenceError::InvalidEvidence("coverage node missing id".into())
        })?;
        let Some(span) = node_span(node) else {
            out.push(NodeCoverage {
                node_id: id,
                measurement: CoverageMeasurement::Unmeasured,
                covered_lines: 0,
                instrumented_lines: 0,
            });
            continue;
        };
        out.push(measure(coverage, &id, &span));
    }
    Ok(out)
}

/// Coverage findings for a dual-revision packet.
///
/// `graph` keys used: `changed`, `risk`, `hot_path`, `new`, `static_selected`,
/// `measured_tests`, `obligations`, `removed_proof_tests`.
///
/// # Errors
///
/// Invalid node identity.
pub fn map_coverage_findings(
    base_cov: Option<&CoverageArtifact>,
    head_cov: Option<&CoverageArtifact>,
    base_graph: &Value,
    head_graph: &Value,
) -> Result<Vec<QualityFinding>, IntelligenceError> {
    let base_nodes = map_coverage_to_nodes(base_cov, base_graph)?;
    let head_nodes = map_coverage_to_nodes(head_cov, head_graph)?;
    let mut findings = Vec::new();
    findings.extend(changed_uncovered(&head_nodes, head_graph));
    findings.extend(impacted_regression(&base_nodes, &head_nodes, head_graph));
    findings.extend(new_unmeasured(&base_nodes, &head_nodes, head_graph));
    findings.extend(static_vs_measured(head_graph));
    findings.extend(hot_path_weak(&head_nodes, head_graph));
    findings.extend(obligation_gaps(head_graph));
    findings.extend(removed_proof_tests(head_graph));
    let _ = base_graph;
    Ok(findings)
}

fn measure(coverage: Option<&CoverageArtifact>, id: &str, span: &NodeSpan) -> NodeCoverage {
    let Some(file) = coverage.and_then(|artifact| {
        artifact.files.iter().find(|file| paths_eq(&file.path, &span.file))
    }) else {
        return NodeCoverage {
            node_id: id.to_owned(),
            measurement: CoverageMeasurement::Unmeasured,
            covered_lines: 0,
            instrumented_lines: 0,
        };
    };
    let mut covered = 0_u64;
    let mut instrumented = 0_u64;
    for range in &file.covered {
        let n = overlap_len(*range, span.start_line, span.end_line);
        covered = covered.saturating_add(n);
        instrumented = instrumented.saturating_add(n);
    }
    for range in &file.uncovered {
        instrumented = instrumented.saturating_add(overlap_len(*range, span.start_line, span.end_line));
    }
    let measurement = if instrumented == 0 {
        CoverageMeasurement::Unmeasured
    } else if covered == 0 {
        CoverageMeasurement::Uncovered
    } else {
        CoverageMeasurement::Covered
    };
    NodeCoverage {
        node_id: id.to_owned(),
        measurement,
        covered_lines: covered,
        instrumented_lines: instrumented,
    }
}

fn changed_uncovered(nodes: &[NodeCoverage], graph: &Value) -> Vec<QualityFinding> {
    let mut out = Vec::new();
    for node in nodes {
        if !flag(graph, &node.node_id, "changed") || !high_risk(graph, &node.node_id) {
            continue;
        }
        if node.measurement != CoverageMeasurement::Uncovered {
            continue;
        }
        out.push(cov_finding(
            "WVQ-COV-001",
            Severity::Warn,
            &node.node_id,
            format!(
                "changed high-risk node {} has measured coverage 0/{} (uncovered, not missing)",
                node.node_id, node.instrumented_lines
            ),
        ));
    }
    out
}

fn impacted_regression(
    base: &[NodeCoverage],
    head: &[NodeCoverage],
    graph: &Value,
) -> Vec<QualityFinding> {
    let mut out = Vec::new();
    for head_node in head {
        if !flag(graph, &head_node.node_id, "changed") {
            continue;
        }
        let Some(base_node) = base.iter().find(|item| item.node_id == head_node.node_id) else {
            continue;
        };
        if base_node.instrumented_lines == 0 || head_node.instrumented_lines == 0 {
            continue;
        }
        let base_m = milles(base_node.covered_lines, base_node.instrumented_lines);
        let head_m = milles(head_node.covered_lines, head_node.instrumented_lines);
        if base_m > head_m && base_m.saturating_sub(head_m) > 20 {
            out.push(cov_finding(
                "WVQ-COV-003",
                Severity::Warn,
                &head_node.node_id,
                format!(
                    "impacted coverage {}/{} → {}/{} (drop over 2%)",
                    base_node.covered_lines,
                    base_node.instrumented_lines,
                    head_node.covered_lines,
                    head_node.instrumented_lines
                ),
            ));
        }
    }
    out
}

fn new_unmeasured(
    base: &[NodeCoverage],
    head: &[NodeCoverage],
    graph: &Value,
) -> Vec<QualityFinding> {
    let mut out = Vec::new();
    for head_node in head {
        if !flag(graph, &head_node.node_id, "new") {
            continue;
        }
        if base.iter().any(|item| item.node_id == head_node.node_id) {
            continue;
        }
        if head_node.measurement != CoverageMeasurement::Unmeasured {
            continue;
        }
        out.push(cov_finding(
            "WVQ-COV-004",
            Severity::Warn,
            &head_node.node_id,
            format!(
                "new region {} is unmeasured (missing evidence, not uncovered)",
                head_node.node_id
            ),
        ));
    }
    out
}

fn static_vs_measured(graph: &Value) -> Vec<QualityFinding> {
    let static_sel = string_set(graph, "static_selected");
    let measured = string_set(graph, "measured_tests");
    if static_sel.is_empty() && measured.is_empty() {
        return Vec::new();
    }
    let only_static = static_sel.difference(&measured).count();
    let only_measured = measured.difference(&static_sel).count();
    if only_static == 0 && only_measured == 0 {
        return Vec::new();
    }
    vec![cov_finding(
        "WVQ-COV-006",
        Severity::Info,
        "static-vs-measured",
        format!(
            "static reachability and measured coverage disagree (static-only {only_static}, measured-only {only_measured}); static is not measured coverage"
        ),
    )]
}

fn hot_path_weak(nodes: &[NodeCoverage], graph: &Value) -> Vec<QualityFinding> {
    let mut out = Vec::new();
    for node in nodes {
        if !flag(graph, &node.node_id, "hot_path") {
            continue;
        }
        if node.instrumented_lines == 0 || node.covered_lines.saturating_mul(2) >= node.instrumented_lines {
            continue;
        }
        out.push(cov_finding(
            "WVQ-COV-007",
            Severity::Warn,
            &node.node_id,
            format!(
                "hot path {} measured {}/{} lines",
                node.node_id, node.covered_lines, node.instrumented_lines
            ),
        ));
    }
    out
}

fn obligation_gaps(graph: &Value) -> Vec<QualityFinding> {
    let mut out = Vec::new();
    let Some(items) = graph.get("obligations").and_then(Value::as_array) else {
        return out;
    };
    for item in items {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        if item.get("has_proof_path") == Some(&Value::Bool(true)) {
            continue;
        }
        out.push(cov_finding(
            "WVQ-COV-002",
            Severity::Error,
            id,
            format!("impacted obligation {id} has no executable proof path"),
        ));
    }
    out
}

fn removed_proof_tests(graph: &Value) -> Vec<QualityFinding> {
    let mut out = Vec::new();
    let Some(items) = graph.get("removed_proof_tests").and_then(Value::as_array) else {
        return out;
    };
    for item in items {
        let Some(id) = item.as_str() else {
            continue;
        };
        out.push(cov_finding(
            "WVQ-COV-005",
            Severity::Error,
            id,
            format!("proof-bearing test {id} removed without replacement"),
        ));
    }
    out
}

struct NodeSpan {
    file: String,
    start_line: u32,
    end_line: u32,
}

fn graph_nodes(graph: &Value) -> &[Value] {
    graph
        .get("nodes")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn node_id(node: &Value) -> Option<String> {
    node.get("id").and_then(Value::as_str).map(str::to_owned)
}

fn node_span(node: &Value) -> Option<NodeSpan> {
    let file = node
        .pointer("/span/file")
        .or_else(|| node.get("file"))
        .and_then(Value::as_str)?;
    let start = node
        .pointer("/span/start_line")
        .or_else(|| node.get("start_line"))
        .and_then(Value::as_u64)?;
    let end = node
        .pointer("/span/end_line")
        .or_else(|| node.get("end_line"))
        .and_then(Value::as_u64)
        .unwrap_or(start);
    Some(NodeSpan {
        file: file.replace('\\', "/"),
        start_line: u32::try_from(start).ok()?,
        end_line: u32::try_from(end).ok()?.max(u32::try_from(start).ok()?),
    })
}

fn flag(graph: &Value, id: &str, key: &str) -> bool {
    graph_nodes(graph)
        .iter()
        .any(|node| node_id(node).as_deref() == Some(id) && node.get(key) == Some(&Value::Bool(true)))
}

fn high_risk(graph: &Value, id: &str) -> bool {
    graph_nodes(graph).iter().any(|node| {
        node_id(node).as_deref() == Some(id)
            && matches!(
                node.get("risk").and_then(Value::as_str),
                Some("high" | "critical")
            )
    })
}

fn string_set(graph: &Value, key: &str) -> std::collections::BTreeSet<String> {
    graph
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn overlap_len(range: LineRange, start: u32, end: u32) -> u64 {
    let lo = range.start.max(start);
    let hi = range.end.min(end);
    if hi < lo {
        0
    } else {
        u64::from(hi.saturating_sub(lo).saturating_add(1))
    }
}

fn paths_eq(left: &str, right: &str) -> bool {
    left.replace('\\', "/") == right.replace('\\', "/")
}

fn milles(covered: u64, instrumented: u64) -> u64 {
    if instrumented == 0 {
        0
    } else {
        covered.saturating_mul(1000) / instrumented
    }
}

fn cov_finding(check: &str, severity: Severity, id: &str, summary: String) -> QualityFinding {
    let mut finding = QualityFinding::new(
        CheckId::new(check).expect("static WVQ coverage check ids are non-empty"),
        severity,
        SubjectRef::GraphNode(id.to_owned()),
        summary,
    );
    finding.weavatrix_fingerprint = Some(id.to_owned());
    finding
}
