//! Historical co-change / regression risk from Weavatrix git evidence.

use serde_json::Value;

use crate::weavatrix::IntelligenceError;
use wvq_domain::{CheckId, QualityFinding, Severity, SubjectRef};

/// Map a history packet (`changed`, `cochange`, `regressions`, `churn`, `reverts`).
///
/// # Errors
///
/// Fails closed when a required path/repo id is missing.
pub fn map_history_risk(report: &Value) -> Result<Vec<QualityFinding>, IntelligenceError> {
    let changed = string_set(report, "changed");
    let mut findings = Vec::new();
    findings.extend(cochange_omissions(report, &changed)?);
    findings.extend(count_findings(report, "regressions", "WVQ-HIST-002", |path, n| {
        format!("changed region {path} has {n} historical regressions")
    })?);
    findings.extend(churn_hotspots(report, &changed)?);
    findings.extend(count_findings(report, "reverts", "WVQ-HIST-004", |path, n| {
        format!("region {path} was reverted {n} times historically")
    })?);
    findings.extend(cross_repo_history(report)?);
    Ok(findings)
}

fn cochange_omissions(
    report: &Value,
    changed: &std::collections::BTreeSet<String>,
) -> Result<Vec<QualityFinding>, IntelligenceError> {
    let mut findings = Vec::new();
    let Some(items) = report.get("cochange").and_then(Value::as_array) else {
        return Ok(findings);
    };
    for item in items {
        let partner = item
            .get("with")
            .and_then(Value::as_str)
            .ok_or_else(|| IntelligenceError::InvalidEvidence("cochange missing with".into()))?;
        let source = item
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("(feature)");
        if !changed.iter().any(|path| path == source || source == "(feature)") {
            continue;
        }
        if changed.contains(partner) {
            continue;
        }
        let count = item.get("count").and_then(Value::as_u64).unwrap_or(0);
        findings.push(hist_finding(
            "WVQ-HIST-001",
            Severity::Warn,
            partner,
            format!("historically co-changes with {source} ({count} times) but was omitted"),
        ));
    }
    Ok(findings)
}

fn churn_hotspots(
    report: &Value,
    changed: &std::collections::BTreeSet<String>,
) -> Result<Vec<QualityFinding>, IntelligenceError> {
    let mut findings = Vec::new();
    let Some(items) = report.get("churn").and_then(Value::as_array) else {
        return Ok(findings);
    };
    for item in items {
        let path = item
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| IntelligenceError::InvalidEvidence("churn missing path".into()))?;
        if !changed.is_empty() && !changed.contains(path) {
            continue;
        }
        let commits = item.get("commits").and_then(Value::as_u64).unwrap_or(0);
        let degree = item.get("degree").and_then(Value::as_u64).unwrap_or(0);
        findings.push(hist_finding(
            "WVQ-HIST-003",
            Severity::Warn,
            path,
            format!("churn hotspot {path}: {commits} commits, degree {degree}, weak proof"),
        ));
    }
    Ok(findings)
}

fn count_findings(
    report: &Value,
    key: &str,
    check: &str,
    summary: fn(&str, u64) -> String,
) -> Result<Vec<QualityFinding>, IntelligenceError> {
    let mut findings = Vec::new();
    let Some(items) = report.get(key).and_then(Value::as_array) else {
        return Ok(findings);
    };
    for item in items {
        let path = item
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                IntelligenceError::InvalidEvidence(format!("{key} missing path"))
            })?;
        let count = item.get("count").and_then(Value::as_u64).unwrap_or(0);
        findings.push(hist_finding(check, Severity::Warn, path, summary(path, count)));
    }
    Ok(findings)
}

fn cross_repo_history(report: &Value) -> Result<Vec<QualityFinding>, IntelligenceError> {
    let mut findings = Vec::new();
    let Some(items) = report.get("cross_repo").and_then(Value::as_array) else {
        return Ok(findings);
    };
    for item in items {
        let repo = item
            .get("repo")
            .and_then(Value::as_str)
            .ok_or_else(|| IntelligenceError::InvalidEvidence("history cross_repo missing repo".into()))?;
        if item.get("verified") == Some(&Value::Bool(true)) {
            continue;
        }
        findings.push(hist_finding(
            "WVQ-HIST-005",
            Severity::Warn,
            repo,
            format!("historically coupled repo {repo} lacks companion change/verification"),
        ));
    }
    Ok(findings)
}

fn string_set(value: &Value, key: &str) -> std::collections::BTreeSet<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn hist_finding(check: &str, severity: Severity, id: &str, summary: String) -> QualityFinding {
    let mut finding = QualityFinding::new(
        CheckId::new(check).expect("static WVQ history check ids are non-empty"),
        severity,
        SubjectRef::File(id.to_owned()),
        summary,
    );
    finding.weavatrix_fingerprint = Some(id.to_owned());
    finding
}
