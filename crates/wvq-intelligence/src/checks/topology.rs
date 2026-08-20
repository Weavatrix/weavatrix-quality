//! Topology drift from Weavatrix `god_nodes` / degree / community evidence.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::weavatrix::IntelligenceError;
use wvq_domain::{CheckId, QualityFinding, Severity, SubjectRef};

#[derive(Clone)]
struct NodeMetrics {
    incoming: u64,
    outgoing: u64,
    degree: u64,
    blast_radius: u64,
    community_span: u64,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CrossEdge {
    from: String,
    to: String,
    from_community: u64,
    to_community: u64,
}

/// Compare base/head topology snapshots. Findings always include both numbers.
///
/// # Errors
///
/// Fails closed when a node or edge is missing identity.
pub fn map_topology_delta(
    base: &Value,
    head: &Value,
) -> Result<Vec<QualityFinding>, IntelligenceError> {
    let base_nodes = index_nodes(base)?;
    let head_nodes = index_nodes(head)?;
    let god_band = u64_field(head, "god_band_degree")
        .or_else(|| u64_field(base, "god_band_degree"))
        .unwrap_or(10);
    let blast_ratio = ratio_millis(head)
        .or_else(|| ratio_millis(base))
        .unwrap_or(2000);
    let mut findings = Vec::new();
    let ids: BTreeSet<_> = base_nodes
        .keys()
        .chain(head_nodes.keys())
        .cloned()
        .collect();
    for id in ids {
        let base_n = base_nodes.get(&id);
        let head_n = head_nodes.get(&id);
        findings.extend(node_findings(&id, base_n, head_n, god_band, blast_ratio));
    }
    findings.extend(community_leaks(base, head)?);
    Ok(findings)
}

fn node_findings(
    id: &str,
    base: Option<&NodeMetrics>,
    head: Option<&NodeMetrics>,
    god_band: u64,
    blast_ratio: u64,
) -> Vec<QualityFinding> {
    let Some(head) = head else {
        return Vec::new();
    };
    let zero = NodeMetrics {
        incoming: 0,
        outgoing: 0,
        degree: 0,
        blast_radius: 0,
        community_span: 0,
    };
    let base = base.unwrap_or(&zero);
    let mut out = Vec::new();
    let entered_god = base.degree < god_band && head.degree >= god_band;
    let god_grew = base.degree >= god_band && head.degree > base.degree;
    if entered_god || god_grew {
        out.push(metric_finding(
            "WVQ-GRAPH-001",
            id,
            format!(
                "god-node growth degree {} → {} (band {god_band})",
                base.degree, head.degree
            ),
        ));
    }
    if head.outgoing > base.outgoing {
        out.push(metric_finding(
            "WVQ-GRAPH-002",
            id,
            format!("fan-out {} → {}", base.outgoing, head.outgoing),
        ));
    }
    if head.incoming > base.incoming {
        out.push(metric_finding(
            "WVQ-GRAPH-003",
            id,
            format!("fan-in {} → {}", base.incoming, head.incoming),
        ));
    }
    if blast_inflated(base.blast_radius, head.blast_radius, blast_ratio) {
        out.push(metric_finding(
            "WVQ-GRAPH-004",
            id,
            format!(
                "blast-radius {} → {} (threshold ×{}.{})",
                base.blast_radius,
                head.blast_radius,
                blast_ratio / 1000,
                blast_ratio % 1000 / 100
            ),
        ));
    }
    if head.community_span >= 2 && head.community_span > base.community_span {
        out.push(metric_finding(
            "WVQ-GRAPH-006",
            id,
            format!(
                "community span {} → {} (centralization)",
                base.community_span, head.community_span
            ),
        ));
    }
    if base.degree > 0 && head.degree == 0 {
        out.push(metric_finding(
            "WVQ-GRAPH-007",
            id,
            format!("structural orphan degree {} → 0", base.degree),
        ));
    }
    out
}

fn blast_inflated(base: u64, head: u64, ratio_millis: u64) -> bool {
    if head <= base {
        return false;
    }
    head.saturating_mul(1000) >= base.saturating_mul(ratio_millis)
}

fn community_leaks(base: &Value, head: &Value) -> Result<Vec<QualityFinding>, IntelligenceError> {
    let base_edges = index_edges(base)?;
    let mut findings = Vec::new();
    for edge in index_edges(head)? {
        if edge.from_community == edge.to_community || base_edges.contains(&edge) {
            continue;
        }
        let id = format!("{}->{}", edge.from, edge.to);
        findings.push(metric_finding(
            "WVQ-GRAPH-005",
            &id,
            format!(
                "community leak {} → {} (communities {} → {})",
                edge.from, edge.to, edge.from_community, edge.to_community
            ),
        ));
    }
    Ok(findings)
}

fn index_nodes(report: &Value) -> Result<BTreeMap<String, NodeMetrics>, IntelligenceError> {
    let mut map = BTreeMap::new();
    for item in node_items(report) {
        let id = node_id(item)
            .ok_or_else(|| IntelligenceError::InvalidEvidence("topology node missing id".into()))?;
        let incoming = u64_field(item, "incoming").unwrap_or(0);
        let outgoing = u64_field(item, "outgoing").unwrap_or(0);
        let degree = u64_field(item, "degree").unwrap_or(incoming.saturating_add(outgoing));
        let blast = u64_field(item, "blast_radius").unwrap_or(incoming);
        map.insert(
            id,
            NodeMetrics {
                incoming,
                outgoing,
                degree,
                blast_radius: blast,
                community_span: u64_field(item, "community_span").unwrap_or(1),
            },
        );
    }
    Ok(map)
}

fn node_items(report: &Value) -> Vec<&Value> {
    report
        .get("nodes")
        .and_then(Value::as_array)
        .or_else(|| report.get("hubs").and_then(Value::as_array))
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn node_id(item: &Value) -> Option<String> {
    item.get("id")
        .and_then(Value::as_str)
        .or_else(|| item.pointer("/node/id").and_then(Value::as_str))
        .map(str::to_owned)
}

fn index_edges(report: &Value) -> Result<BTreeSet<CrossEdge>, IntelligenceError> {
    let mut edges = BTreeSet::new();
    let Some(items) = report.get("edges").and_then(Value::as_array) else {
        return Ok(edges);
    };
    for item in items {
        let from = item.get("from").and_then(Value::as_str).ok_or_else(|| {
            IntelligenceError::InvalidEvidence("topology edge missing from".into())
        })?;
        let to = item
            .get("to")
            .and_then(Value::as_str)
            .ok_or_else(|| IntelligenceError::InvalidEvidence("topology edge missing to".into()))?;
        let from_community = u64_field(item, "from_community").unwrap_or(0);
        let to_community = u64_field(item, "to_community").unwrap_or(0);
        edges.insert(CrossEdge {
            from: from.to_owned(),
            to: to.to_owned(),
            from_community,
            to_community,
        });
    }
    Ok(edges)
}

fn metric_finding(check: &str, id: &str, summary: String) -> QualityFinding {
    let mut finding = QualityFinding::new(
        CheckId::new(check).expect("static WVQ graph check ids are non-empty"),
        graph_severity(check),
        SubjectRef::GraphNode(id.to_owned()),
        summary,
    );
    finding.weavatrix_fingerprint = Some(format!("{check}:{id}"));
    finding
}

fn graph_severity(check: &str) -> Severity {
    if check == "WVQ-GRAPH-005" {
        Severity::Info
    } else {
        Severity::Warn
    }
}

fn u64_field(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn ratio_millis(value: &Value) -> Option<u64> {
    let item = value.get("blast_radius_growth_ratio")?;
    if let Some(whole) = item.as_u64() {
        return Some(whole.saturating_mul(1000));
    }
    let raw = item.to_string();
    let (whole, frac) = raw.split_once('.').unwrap_or((raw.as_str(), "0"));
    let whole = whole.parse::<u64>().ok()?;
    let frac = frac.chars().take(3).collect::<String>();
    let frac = format!("{frac:0<3}").parse::<u64>().unwrap_or(0);
    Some(whole.saturating_mul(1000).saturating_add(frac))
}
