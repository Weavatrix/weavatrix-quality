//! Task 8: topology drift always quotes base/head numbers.

use serde_json::json;
use wvq_intelligence::{DebtBaseline, gate_topology};

fn node(
    id: &str,
    incoming: u64,
    outgoing: u64,
    blast_radius: u64,
    community_id: u64,
) -> serde_json::Value {
    json!({
        "id": id,
        "incoming": incoming,
        "outgoing": outgoing,
        "degree": incoming + outgoing,
        "blast_radius": blast_radius,
        "community_id": community_id,
        "community_span": 1
    })
}

#[test]
fn fan_out_growth_includes_base_and_head_counts() {
    let base = json!({
        "god_band_degree": 10,
        "nodes": [node("file:src/util.js", 1, 2, 1, 0)]
    });
    let head = json!({
        "god_band_degree": 10,
        "nodes": [node("file:src/util.js", 1, 8, 1, 0)]
    });
    let delta = gate_topology(&base, &head, &DebtBaseline::default()).unwrap();
    let finding = delta
        .new
        .iter()
        .find(|item| item.check.as_str() == "WVQ-GRAPH-002")
        .expect("fan-out finding");
    assert!(finding.summary.contains("2 → 8"), "{}", finding.summary);
}

#[test]
fn reverse_blast_radius_growth_uses_numeric_ratio() {
    let base = json!({
        "blast_radius_growth_ratio": 2.0,
        "nodes": [node("file:src/core.js", 4, 1, 4, 0)]
    });
    let head = json!({
        "blast_radius_growth_ratio": 2.0,
        "nodes": [node("file:src/core.js", 12, 1, 12, 0)]
    });
    let delta = gate_topology(&base, &head, &DebtBaseline::default()).unwrap();
    let finding = delta
        .new
        .iter()
        .find(|item| item.check.as_str() == "WVQ-GRAPH-004")
        .expect("blast-radius finding");
    assert!(finding.summary.contains("4 → 12"), "{}", finding.summary);
    assert!(
        finding.summary.contains("threshold ×2.0"),
        "{}",
        finding.summary
    );
}

#[test]
fn new_community_crossing_edge_is_reported() {
    let base = json!({
        "nodes": [node("file:src/ui.js", 1, 1, 1, 0)],
        "edges": [{ "from": "file:src/ui.js", "to": "file:src/view.js", "from_community": 0, "to_community": 0 }]
    });
    let head = json!({
        "nodes": [node("file:src/ui.js", 1, 2, 1, 0)],
        "edges": [
            { "from": "file:src/ui.js", "to": "file:src/view.js", "from_community": 0, "to_community": 0 },
            { "from": "file:src/ui.js", "to": "file:src/db.js", "from_community": 0, "to_community": 2 }
        ]
    });
    let delta = gate_topology(&base, &head, &DebtBaseline::default()).unwrap();
    let finding = delta
        .new
        .iter()
        .find(|item| item.check.as_str() == "WVQ-GRAPH-005")
        .expect("community leak");
    assert!(finding.summary.contains("0 → 2"), "{}", finding.summary);
    assert_eq!(finding.severity, wvq_domain::Severity::Info);
}

#[test]
fn god_node_growth_quotes_degree_and_band() {
    let base = json!({
        "god_band_degree": 10,
        "nodes": [node("file:src/hub.js", 3, 3, 3, 0)]
    });
    let head = json!({
        "god_band_degree": 10,
        "nodes": [node("file:src/hub.js", 9, 8, 9, 0)]
    });
    let delta = gate_topology(&base, &head, &DebtBaseline::default()).unwrap();
    let finding = delta
        .new
        .iter()
        .find(|item| item.check.as_str() == "WVQ-GRAPH-001")
        .expect("god-node finding");
    assert!(finding.summary.contains("6 → 17"), "{}", finding.summary);
    assert!(finding.summary.contains("band 10"), "{}", finding.summary);
}
