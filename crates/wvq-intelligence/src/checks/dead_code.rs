//! Dead-code delta from Weavatrix `find_dead_code`. Never suggests deletion.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::weavatrix::IntelligenceError;
use wvq_domain::{CheckId, QualityFinding, Severity, SubjectRef};

/// Map one `find_dead_code` report into findings.
///
/// `prior_dead` / `prior_live` are node ids from the other revision.
///
/// # Errors
///
/// Fails closed when a candidate has no node id.
pub fn map_dead_code_report(
    report: &Value,
    prior_dead: &BTreeSet<String>,
    prior_live: &BTreeSet<String>,
) -> Result<Vec<QualityFinding>, IntelligenceError> {
    let Some(candidates) = report.get("candidates").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut findings = Vec::new();
    for candidate in candidates {
        findings.push(map_candidate(candidate, prior_dead, prior_live)?);
    }
    Ok(findings)
}

/// Node ids listed as dead in a `find_dead_code` report.
#[must_use]
pub fn dead_ids(report: &Value) -> BTreeSet<String> {
    report
        .get("candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(node_id)
        .collect()
}

/// Optional live-id set (`live_nodes`) from a dual-revision packet.
#[must_use]
pub fn live_ids(report: &Value) -> BTreeSet<String> {
    report
        .get("live_nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn map_candidate(
    candidate: &Value,
    prior_dead: &BTreeSet<String>,
    prior_live: &BTreeSet<String>,
) -> Result<QualityFinding, IntelligenceError> {
    let id = node_id(candidate).ok_or_else(|| {
        IntelligenceError::InvalidEvidence("dead-code candidate missing node.id".into())
    })?;
    let path = node_path(candidate);
    let orphaned = !prior_dead.contains(&id)
        && (prior_live.contains(&id) || candidate.get("prior_reachable") == Some(&Value::Bool(true)));
    let (check, severity) = if orphaned {
        (static_check("WVQ-DEAD-002"), Severity::Warn)
    } else if is_test_path(&path) {
        (static_check("WVQ-DEAD-005"), Severity::Info)
    } else if is_public_surface(candidate) {
        (static_check("WVQ-DEAD-003"), Severity::Warn)
    } else {
        (static_check("WVQ-DEAD-001"), Severity::Warn)
    };
    let subject = if path.is_empty() {
        SubjectRef::Symbol(node_label(candidate).unwrap_or_else(|| id.clone()))
    } else {
        SubjectRef::File(path)
    };
    let mut finding = QualityFinding::new(check, severity, subject, dead_summary(candidate));
    finding.weavatrix_fingerprint = Some(id);
    Ok(finding)
}

fn dead_summary(candidate: &Value) -> String {
    let reason = candidate
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("unreachable from declared entry points");
    let caveat = candidate
        .get("caveat")
        .and_then(Value::as_str)
        .unwrap_or("framework, reflection, public API, runtime and generated use may be invisible");
    format!("{reason} [uncertainty: {caveat}]")
}

fn node_id(candidate: &Value) -> Option<String> {
    candidate
        .pointer("/node/id")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn node_label(candidate: &Value) -> Option<String> {
    candidate
        .pointer("/node/label")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn node_path(candidate: &Value) -> String {
    candidate
        .pointer("/node/span/file")
        .or_else(|| candidate.pointer("/node/attributes/path"))
        .and_then(Value::as_str)
        .or_else(|| {
            candidate
                .pointer("/node/id")
                .and_then(Value::as_str)
                .and_then(|id| id.split(':').nth(1))
        })
        .unwrap_or("")
        .replace('\\', "/")
}

fn is_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains("/test/")
        || lower.contains("/tests/")
        || lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.contains("_test.")
}

fn is_public_surface(candidate: &Value) -> bool {
    let exported = candidate.pointer("/node/attributes/exported");
    if exported == Some(&Value::Bool(true)) || exported.and_then(Value::as_str) == Some("true") {
        return true;
    }
    matches!(
        candidate
            .pointer("/node/attributes/visibility")
            .and_then(Value::as_str),
        Some("public" | "exported")
    )
}

fn static_check(id: &str) -> CheckId {
    CheckId::new(id).expect("static WVQ check ids are non-empty")
}
