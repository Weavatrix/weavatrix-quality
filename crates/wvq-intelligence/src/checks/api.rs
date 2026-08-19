//! API/transport drift vs `OpenSpec` surface and Proof coverage.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::weavatrix::IntelligenceError;
use wvq_domain::{CheckId, QualityFinding, Severity, SubjectRef};

/// Compare Weavatrix `list_endpoints` base/head with `OpenSpec` + Proof sets.
///
/// `spec` keys: `removed`, `added`, `no_spec_rationale`.
/// `proofs` keys: `proven`, `impacted` (string or `{id, risk}`).
///
/// # Errors
///
/// Fails closed when an endpoint object has no id/label.
pub fn map_api_delta(
    base: &Value,
    head: &Value,
    spec: &Value,
    proofs: &Value,
) -> Result<Vec<QualityFinding>, IntelligenceError> {
    let base_eps = endpoint_map(base)?;
    let head_eps = endpoint_map(head)?;
    let removed_spec = string_set(spec, "removed");
    let added_spec = string_set(spec, "added");
    let rationale = string_set(spec, "no_spec_rationale");
    let proven = string_set(proofs, "proven");
    let mut findings = Vec::new();

    for id in base_eps.keys() {
        if head_eps.contains_key(id) {
            continue;
        }
        if removed_spec.contains(id) {
            continue;
        }
        findings.push(api_finding(
            "WVQ-API-001",
            Severity::Error,
            id,
            format!("endpoint {id} removed with no OpenSpec REMOVED requirement"),
        ));
    }

    for id in head_eps.keys() {
        if base_eps.contains_key(id) || added_spec.contains(id) || rationale.contains(id) {
            continue;
        }
        findings.push(api_finding(
            "WVQ-API-002",
            Severity::Warn,
            id,
            format!("endpoint {id} added without OpenSpec capability or no-spec rationale"),
        ));
    }

    findings.extend(event_drift(base, head));
    findings.extend(cross_repo(head)?);
    findings.extend(handler_drift(base, head, &proven));
    findings.extend(unproven_impact(proofs, &proven)?);
    Ok(findings)
}

fn endpoint_map(report: &Value) -> Result<BTreeMap<String, Value>, IntelligenceError> {
    let mut map = BTreeMap::new();
    let Some(items) = report.get("endpoints").and_then(Value::as_array) else {
        return Ok(map);
    };
    for item in items {
        let id = endpoint_id(item).ok_or_else(|| {
            IntelligenceError::InvalidEvidence("endpoint missing id/label".into())
        })?;
        map.insert(id, item.clone());
    }
    Ok(map)
}

fn endpoint_id(item: &Value) -> Option<String> {
    item.as_str()
        .or_else(|| item.get("label").and_then(Value::as_str))
        .or_else(|| item.get("id").and_then(Value::as_str))
        .map(str::to_owned)
}

fn event_drift(base: &Value, head: &Value) -> Vec<QualityFinding> {
    let mut findings = Vec::new();
    for side in ["producers", "consumers"] {
        let base_set = string_set(base.get("events").unwrap_or(&Value::Null), side);
        let head_set = string_set(head.get("events").unwrap_or(&Value::Null), side);
        for topic in base_set.symmetric_difference(&head_set) {
            findings.push(api_finding(
                "WVQ-API-003",
                Severity::Warn,
                topic,
                format!("event {side} drift on {topic} without companion proof"),
            ));
        }
    }
    findings
}

fn cross_repo(head: &Value) -> Result<Vec<QualityFinding>, IntelligenceError> {
    let mut findings = Vec::new();
    let Some(items) = head.get("cross_repo").and_then(Value::as_array) else {
        return Ok(findings);
    };
    for item in items {
        let repo = item
            .get("repo")
            .and_then(Value::as_str)
            .ok_or_else(|| IntelligenceError::InvalidEvidence("cross_repo missing repo".into()))?;
        if item.get("verified") == Some(&Value::Bool(true)) {
            continue;
        }
        findings.push(api_finding(
            "WVQ-API-004",
            Severity::Warn,
            repo,
            format!("dependent repo {repo} is impacted without companion verification"),
        ));
    }
    Ok(findings)
}

fn handler_drift(
    base: &Value,
    head: &Value,
    proven: &BTreeSet<String>,
) -> Vec<QualityFinding> {
    let base_h = digest_map(base, "handlers");
    let head_h = digest_map(head, "handlers");
    let mut findings = Vec::new();
    for (id, head_digest) in &head_h {
        let Some(base_digest) = base_h.get(id) else {
            continue;
        };
        if base_digest == head_digest || proven.contains(id) {
            continue;
        }
        findings.push(api_finding(
            "WVQ-API-005",
            Severity::Warn,
            id,
            format!("handler graph for {id} changed; existing proof does not cover the new path"),
        ));
    }
    findings
}

fn unproven_impact(
    proofs: &Value,
    proven: &BTreeSet<String>,
) -> Result<Vec<QualityFinding>, IntelligenceError> {
    let mut findings = Vec::new();
    let Some(items) = proofs.get("impacted").and_then(Value::as_array) else {
        return Ok(findings);
    };
    for item in items {
        let (id, high) = impacted_entry(item)?;
        if proven.contains(&id) {
            continue;
        }
        let severity = if high {
            Severity::Error
        } else {
            Severity::Warn
        };
        findings.push(api_finding(
            "WVQ-API-006",
            severity,
            &id,
            format!("impacted contract {id} has no runtime Proof"),
        ));
    }
    Ok(findings)
}

fn impacted_entry(item: &Value) -> Result<(String, bool), IntelligenceError> {
    if let Some(id) = item.as_str() {
        return Ok((id.to_owned(), false));
    }
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| IntelligenceError::InvalidEvidence("impacted entry missing id".into()))?;
    let risk = item.get("risk").and_then(Value::as_str).unwrap_or("");
    Ok((
        id.to_owned(),
        matches!(risk, "high" | "critical"),
    ))
}

fn digest_map(report: &Value, key: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let Some(object) = report.get(key).and_then(Value::as_object) else {
        return map;
    };
    for (id, value) in object {
        if let Some(digest) = value.as_str() {
            map.insert(id.clone(), digest.to_owned());
        }
    }
    map
}

fn string_set(value: &Value, key: &str) -> BTreeSet<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn api_finding(check: &str, severity: Severity, id: &str, summary: String) -> QualityFinding {
    let mut finding = QualityFinding::new(
        CheckId::new(check).expect("static WVQ API check ids are non-empty"),
        severity,
        SubjectRef::Endpoint(id.to_owned()),
        summary,
    );
    finding.weavatrix_fingerprint = Some(id.to_owned());
    finding
}
