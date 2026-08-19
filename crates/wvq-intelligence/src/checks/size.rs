//! Size-growth interpretation of Weavatrix file/function LOC evidence.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::weavatrix::IntelligenceError;
use wvq_domain::{CheckId, QualityFinding, Severity, SubjectRef};

/// Emit `WVQ-SIZE-002` / `WVQ-SIZE-005` when an already-over-budget unit grows.
///
/// # Errors
///
/// Returns [`IntelligenceError::InvalidEvidence`] when a budget violation
/// cannot be read.
pub fn size_growth_findings(
    base: &Value,
    head: &Value,
) -> Result<Vec<QualityFinding>, IntelligenceError> {
    let mut out = Vec::new();
    let base_files = loc_index(base, "file_loc", file_key)?;
    let head_files = loc_index(head, "file_loc", file_key)?;
    for (path, head_loc) in &head_files {
        let Some(base_loc) = base_files.get(path) else {
            continue;
        };
        if head_loc > base_loc {
            out.push(QualityFinding::new(
                static_check("WVQ-SIZE-002"),
                Severity::Warn,
                SubjectRef::File(path.clone()),
                format!("oversized file {path} grew from {base_loc} to {head_loc} loc"),
            ));
        }
    }

    let base_functions = loc_index(base, "function_loc", function_key)?;
    let head_functions = loc_index(head, "function_loc", function_key)?;
    for (key, head_loc) in &head_functions {
        let Some(base_loc) = base_functions.get(key) else {
            continue;
        };
        if head_loc > base_loc {
            out.push(QualityFinding::new(
                static_check("WVQ-SIZE-005"),
                Severity::Warn,
                SubjectRef::Symbol(key.clone()),
                format!("oversized function {key} grew from {base_loc} to {head_loc} loc"),
            ));
        }
    }
    Ok(out)
}

fn loc_index(
    report: &Value,
    kind: &str,
    key: fn(&Value) -> Option<String>,
) -> Result<BTreeMap<String, u64>, IntelligenceError> {
    let mut map = BTreeMap::new();
    for item in raw_violations(report) {
        if item.pointer("/evidence/kind").and_then(Value::as_str) != Some(kind) {
            continue;
        }
        let Some(name) = key(item) else {
            continue;
        };
        let Some(actual) = item.pointer("/evidence/actual").and_then(Value::as_u64) else {
            return Err(IntelligenceError::InvalidEvidence(format!(
                "{kind} violation missing actual loc"
            )));
        };
        map.insert(name, actual);
    }
    Ok(map)
}

fn raw_violations(report: &Value) -> Vec<&Value> {
    let mut items = Vec::new();
    for bucket in ["new", "existing", "warnings", "excepted"] {
        if let Some(array) = report.get(bucket).and_then(Value::as_array) {
            items.extend(array.iter().filter(|item| item.is_object()));
        }
    }
    items
}

fn file_key(item: &Value) -> Option<String> {
    item.pointer("/evidence/file")
        .and_then(Value::as_str)
        .map(|path| path.replace('\\', "/"))
}

fn function_key(item: &Value) -> Option<String> {
    let file = item.pointer("/evidence/file").and_then(Value::as_str)?;
    let symbol = item.pointer("/evidence/symbol").and_then(Value::as_str)?;
    Some(format!("{file}::{symbol}"))
}

fn static_check(id: &str) -> CheckId {
    CheckId::new(id).expect("static WVQ check ids are non-empty")
}
