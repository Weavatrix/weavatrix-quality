//! Quality gates derived from Weavatrix evidence. No second code graph.

mod architecture;
mod size;

use serde_json::Value;

use crate::debt::{DebtBaseline, DebtDelta, classify_debt};
use crate::weavatrix::IntelligenceError;
use wvq_domain::QualityFinding;

pub use architecture::map_architecture_violation;
pub use size::size_growth_findings;

/// Map a `verify_architecture` report into WVQ findings and ratchet base/head.
///
/// # Errors
///
/// Returns [`IntelligenceError::InvalidEvidence`] when a violation is missing
/// its Weavatrix fingerprint or cannot be typed.
pub fn gate_architecture(
    base: &Value,
    head: &Value,
    baseline: &DebtBaseline,
) -> Result<DebtDelta, IntelligenceError> {
    let base_findings = map_architecture_report(base)?;
    let mut head_findings = map_architecture_report(head)?;
    head_findings.extend(size_growth_findings(base, head)?);
    Ok(classify_debt(&base_findings, &head_findings, baseline))
}

/// Flatten Weavatrix `new` / `existing` / `warnings` / `excepted` buckets.
///
/// # Errors
///
/// Returns [`IntelligenceError::InvalidEvidence`] for unusable violations.
pub fn map_architecture_report(report: &Value) -> Result<Vec<QualityFinding>, IntelligenceError> {
    if report.get("state").and_then(Value::as_str) == Some("NOT_CONFIGURED") {
        return Ok(Vec::new());
    }
    let mut findings = Vec::new();
    for key in ["new", "existing", "warnings", "excepted"] {
        let Some(items) = report.get(key).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            if item.is_string() {
                continue;
            }
            findings.push(map_architecture_violation(item)?);
        }
    }
    Ok(findings)
}
