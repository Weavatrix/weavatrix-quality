//! Map Weavatrix Architecture Firewall violations to `WVQ-ARCH-*` / size IDs.

use serde_json::Value;

use crate::weavatrix::IntelligenceError;
use wvq_domain::{CheckId, QualityFinding, Severity, SubjectRef};

/// Map one structured Weavatrix violation into a WVQ finding.
///
/// # Errors
///
/// Fails closed when `fingerprint` is missing.
pub fn map_architecture_violation(violation: &Value) -> Result<QualityFinding, IntelligenceError> {
    let fingerprint = violation
        .get("fingerprint")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            IntelligenceError::InvalidEvidence("architecture violation missing fingerprint".into())
        })?;
    let (check, severity) = classify_violation(violation);
    let subject = subject_of(violation);
    let summary = summary_of(violation, check.as_str());
    let mut finding = QualityFinding::new(check, severity, subject, summary);
    finding.weavatrix_fingerprint = Some(fingerprint.to_owned());
    Ok(finding)
}

fn classify_violation(violation: &Value) -> (CheckId, Severity) {
    let evidence_kind = violation
        .pointer("/evidence/kind")
        .and_then(Value::as_str)
        .unwrap_or("");
    let rule_id = violation
        .pointer("/rule/id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let rule_severity = violation
        .pointer("/rule/severity")
        .and_then(Value::as_str)
        .unwrap_or("");

    if evidence_kind == "file_loc" || rule_id == "budget.maxFileLoc" {
        return (check("WVQ-SIZE-001"), Severity::Warn);
    }
    if evidence_kind == "function_loc" || rule_id == "budget.maxFunctionLoc" {
        return (check("WVQ-SIZE-004"), Severity::Warn);
    }
    if evidence_kind == "runtime_cycles" || rule_id == "budget.runtimeCycles" {
        return (check("WVQ-ARCH-003"), Severity::Error);
    }
    if evidence_kind == "unresolved"
        || violation
            .pointer("/edge/kind")
            .and_then(Value::as_str)
            == Some("unresolved")
    {
        return (check("WVQ-ARCH-004"), Severity::Error);
    }
    if evidence_kind == "dependency_outside_allow_list" {
        let target = violation
            .pointer("/evidence/target_component")
            .and_then(Value::as_str)
            .unwrap_or("");
        if target == "(unmapped)" {
            return (check("WVQ-ARCH-007"), Severity::Warn);
        }
        return (check("WVQ-ARCH-005"), Severity::Warn);
    }
    if rule_severity == "warn" {
        return (check("WVQ-ARCH-002"), Severity::Warn);
    }
    (check("WVQ-ARCH-001"), Severity::Error)
}

fn subject_of(violation: &Value) -> SubjectRef {
    if let Some(file) = violation.pointer("/evidence/file").and_then(Value::as_str) {
        return SubjectRef::File(file.replace('\\', "/"));
    }
    if let Some(path) = violation
        .pointer("/source/span/file")
        .and_then(Value::as_str)
        .or_else(|| violation.pointer("/source/label").and_then(Value::as_str))
        .or_else(|| violation.pointer("/source/id").and_then(Value::as_str))
    {
        let path = path.strip_prefix("file:").unwrap_or(path);
        return SubjectRef::File(path.replace('\\', "/"));
    }
    if let Some(fingerprint) = violation.get("fingerprint").and_then(Value::as_str) {
        return SubjectRef::GraphNode(fingerprint.to_owned());
    }
    SubjectRef::GraphNode("unknown".into())
}

fn summary_of(violation: &Value, check: &str) -> String {
    let file = violation
        .pointer("/evidence/file")
        .and_then(Value::as_str)
        .unwrap_or("");
    let actual = violation.pointer("/evidence/actual");
    let maximum = violation
        .pointer("/evidence/maximum")
        .or_else(|| violation.pointer("/rule/maximum"));
    match check {
        "WVQ-SIZE-001" => format!(
            "file {file} loc {} exceeds maximum {}",
            display_num(actual),
            display_num(maximum)
        ),
        "WVQ-SIZE-004" => format!(
            "function loc {} exceeds maximum {}",
            display_num(actual),
            display_num(maximum)
        ),
        "WVQ-ARCH-003" => format!(
            "runtime cycle count {} exceeds maximum {}",
            display_num(actual),
            display_num(maximum)
        ),
        "WVQ-ARCH-002" => "warn-severity architecture rule violation".into(),
        "WVQ-ARCH-004" => "unresolved local import".into(),
        "WVQ-ARCH-005" => "layer bypass / allow-list miss".into(),
        "WVQ-ARCH-007" => "dependency on unmapped architecture target".into(),
        _ => format!(
            "architecture rule {} violated",
            violation
                .pointer("/rule/id")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ),
    }
}

fn display_num(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_u64)
        .map_or_else(|| "?".into(), |n| n.to_string())
}

fn check(id: &str) -> CheckId {
    CheckId::new(id).expect("static WVQ check ids are non-empty")
}
