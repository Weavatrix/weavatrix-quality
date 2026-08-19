//! Quality gates derived from Weavatrix evidence. No second code graph.

mod api;
mod architecture;
mod dead_code;
mod duplicates;
mod history;
mod size;
mod topology;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::debt::{DebtBaseline, DebtDelta, classify_debt};
use crate::weavatrix::IntelligenceError;
use wvq_domain::{CheckId, FindingState, QualityFinding, Severity};

pub use api::map_api_delta;
pub use architecture::map_architecture_violation;
pub use history::map_history_risk;
pub use dead_code::{dead_ids, live_ids, map_dead_code_report};
pub use duplicates::{family_sizes, map_duplicates_report};
pub use size::size_growth_findings;
pub use topology::map_topology_delta;

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

/// Ratchet `find_dead_code` base/head reports.
///
/// # Errors
///
/// Returns [`IntelligenceError::InvalidEvidence`] when a candidate has no node id.
pub fn gate_dead_code(
    base: &Value,
    head: &Value,
    baseline: &DebtBaseline,
) -> Result<DebtDelta, IntelligenceError> {
    let prior_dead = dead_ids(base);
    let prior_live = live_ids(base);
    let base_findings = map_dead_code_report(base, &BTreeSet::new(), &BTreeSet::new())?;
    let head_findings = map_dead_code_report(head, &prior_dead, &prior_live)?;
    Ok(relabel_returned(
        classify_debt(&base_findings, &head_findings, baseline),
        "WVQ-DEAD-",
        "WVQ-DEAD-004",
        Severity::Error,
    ))
}

/// Ratchet `find_duplicates` base/head reports.
///
/// # Errors
///
/// Returns [`IntelligenceError::InvalidEvidence`] when a family has no id.
pub fn gate_clones(
    base: &Value,
    head: &Value,
    baseline: &DebtBaseline,
) -> Result<DebtDelta, IntelligenceError> {
    let prior = family_sizes(base);
    let base_findings = map_duplicates_report(base, &BTreeMap::new())?;
    let head_findings = map_duplicates_report(head, &prior)?;
    Ok(relabel_returned(
        classify_debt(&base_findings, &head_findings, baseline),
        "WVQ-CLONE-",
        "WVQ-CLONE-005",
        Severity::Error,
    ))
}

/// Compare Weavatrix endpoints with `OpenSpec` + Proof coverage.
///
/// # Errors
///
/// Returns [`IntelligenceError::InvalidEvidence`] when an endpoint has no id.
pub fn gate_api(
    base: &Value,
    head: &Value,
    spec: &Value,
    proofs: &Value,
    baseline: &DebtBaseline,
) -> Result<DebtDelta, IntelligenceError> {
    let findings = map_api_delta(base, head, spec, proofs)?;
    Ok(classify_debt(&[], &findings, baseline))
}

/// Map git/history evidence into advisory findings.
///
/// # Errors
///
/// Returns [`IntelligenceError::InvalidEvidence`] when a path/repo id is missing.
pub fn gate_history(
    report: &Value,
    baseline: &DebtBaseline,
) -> Result<DebtDelta, IntelligenceError> {
    let findings = map_history_risk(report)?;
    Ok(classify_debt(&[], &findings, baseline))
}

/// Compare Weavatrix topology snapshots (hubs, degrees, community edges).
///
/// # Errors
///
/// Returns [`IntelligenceError::InvalidEvidence`] when a node or edge has no id.
pub fn gate_topology(
    base: &Value,
    head: &Value,
    baseline: &DebtBaseline,
) -> Result<DebtDelta, IntelligenceError> {
    let findings = map_topology_delta(base, head)?;
    Ok(classify_debt(&[], &findings, baseline))
}

fn relabel_returned(
    mut delta: DebtDelta,
    prefix: &str,
    returned_id: &str,
    severity: Severity,
) -> DebtDelta {
    for finding in &mut delta.returned {
        if finding.check.as_str().starts_with(prefix) {
            finding.check = CheckId::new(returned_id).expect("static returned check id");
            finding.severity = severity;
            finding.state = FindingState::Returned;
        }
    }
    delta
}
