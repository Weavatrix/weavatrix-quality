//! Extracted command-bus helper.

use super::access::*;

use super::policy::load_test_bindings;

pub(in crate::service) fn parse_axis_state(token: Option<&str>) -> Result<AxisState, BusError> {
    match token {
        Some("not_applicable") => Ok(AxisState::NotApplicable),
        Some("clean") => Ok(AxisState::Clean),
        Some("warnings") => Ok(AxisState::Warnings),
        Some("blocking") => Ok(AxisState::Blocking),
        Some("unmeasured") => Ok(AxisState::Unmeasured),
        other => Err(BusError::Store(format!(
            "unknown ui-integrity axis state `{}`",
            other.unwrap_or("<missing>")
        ))),
    }
}

pub(in crate::service) fn parse_ui_findings(document: &Value, field: &str) -> Result<Vec<UiFindingRef>, BusError> {
    let mut out = Vec::new();
    for value in values_at(document, &format!("/{field}")) {
        let severity = match value.get("severity").and_then(Value::as_str) {
            Some("info") => Severity::Info,
            Some("warn") => Severity::Warn,
            Some("error") => Severity::Error,
            other => {
                return Err(BusError::Store(format!(
                    "ui-integrity finding has unknown severity `{}`",
                    other.unwrap_or("<missing>")
                )));
            }
        };
        out.push(UiFindingRef {
            check: json_string(value, "check")?,
            severity,
            subject: json_string(value, "subject")?,
            route: json_string(value, "route")?,
            viewport: json_string(value, "viewport")?,
            detail: json_string(value, "detail")?,
        });
    }
    Ok(out)
}

pub(in crate::service) fn json_string(value: &Value, field: &str) -> Result<String, BusError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| BusError::Store(format!("ui-integrity finding omitted {field}")))
}

pub(in crate::service) fn json_u64(value: &Value, field: &str) -> u64 {
    value.get(field).and_then(Value::as_u64).unwrap_or_default()
}

/// Repository test paths bound to a high or critical obligation.
pub(in crate::service) fn mandatory_test_paths(repo: &Path, compiled: &Compiled) -> BTreeSet<String> {
    let mandatory = compiled
        .obligations
        .iter()
        .filter(|item| matches!(item.risk, RiskLevel::High | RiskLevel::Critical))
        .map(|item| item.id.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    if mandatory.is_empty() {
        return BTreeSet::new();
    }
    load_test_bindings(repo)
        .unwrap_or_default()
        .into_iter()
        .filter(|binding| {
            binding
                .obligations
                .iter()
                .any(|obligation| mandatory.contains(obligation))
        })
        .map(|binding| binding.path)
        .collect()
}
