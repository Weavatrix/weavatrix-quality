//! Axe-core / Storybook a11y → WVQ findings.
//!
//! This is an adapter, not a port. MPL axe-core stays outside Rust. The
//! producer emits JSON; this module keeps rule id, impact, and bounded
//! selectors, drops HTML, and hands the ratchet an ordinary finding.
//! Severity comes from impact, not from whether a sealed oracle named the node.

use serde::Deserialize;
use serde_json::Value;
use wvq_domain::Severity;

use crate::finding::{UiCheck, UiEvidence, UiIntegrityFinding};
use crate::snapshot::LayoutSnapshot;
use crate::UiError;

/// Ceiling on imported violations in one report.
pub const MAX_A11Y_IMPORT_VIOLATIONS: usize = 128;
/// Ceiling on nodes kept per violation.
pub const MAX_A11Y_IMPORT_NODES: usize = 8;
/// Longest rule id or selector kept in a subject.
pub const MAX_A11Y_IMPORT_TOKEN: usize = 120;

/// Normalise an axe-core or Storybook a11y report into ratchet findings.
///
/// Extra fields (`html`, `failureSummary`, passes) are ignored. A truncated
/// report is still findings, never a clean empty set pretending to be complete.
///
/// # Errors
///
/// The value is not a JSON object.
pub fn import_a11y_violations(
    snapshot: &LayoutSnapshot,
    report: &Value,
) -> Result<(Vec<UiIntegrityFinding>, bool), UiError> {
    let parsed: AxeShaped = serde_json::from_value(report.clone()).map_err(|error| {
        UiError::Malformed(format!("a11y import is not an axe/Storybook report: {error}"))
    })?;
    let producer = parsed
        .producer
        .as_deref()
        .or(parsed.engine.as_deref())
        .unwrap_or(if parsed.results.is_some() {
            "storybook-a11y"
        } else {
            "axe-core"
        });
    let mut violations = parsed.violations;
    if let Some(results) = parsed.results {
        if violations.is_empty() {
            violations = results.violations;
        }
    }
    let truncated = violations.len() > MAX_A11Y_IMPORT_VIOLATIONS;
    violations.truncate(MAX_A11Y_IMPORT_VIOLATIONS);
    let mut findings = Vec::new();
    for violation in violations {
        let rule = bound_token(&violation.id);
        if rule.is_empty() {
            continue;
        }
        let severity = impact_severity(violation.impact.as_deref());
        let node_count = u32::try_from(violation.nodes.len()).unwrap_or(u32::MAX);
        let mut nodes = violation.nodes;
        if nodes.len() > MAX_A11Y_IMPORT_NODES {
            nodes.truncate(MAX_A11Y_IMPORT_NODES);
        }
        if nodes.is_empty() {
            findings.push(finding(
                snapshot,
                producer,
                &rule,
                "page",
                severity,
                0,
            ));
            continue;
        }
        for node in nodes {
            let target = node
                .target
                .iter()
                .map(|item| bound_token(item))
                .find(|item| !item.is_empty())
                .unwrap_or_else(|| "page".into());
            findings.push(finding(
                snapshot,
                producer,
                &rule,
                &target,
                severity,
                node_count,
            ));
        }
    }
    crate::sort_findings(&mut findings);
    Ok((findings, truncated))
}

#[derive(Debug, Deserialize, Default)]
struct AxeShaped {
    #[serde(default)]
    producer: Option<String>,
    #[serde(default)]
    engine: Option<String>,
    #[serde(default)]
    violations: Vec<AxeViolation>,
    #[serde(default)]
    results: Option<AxeResults>,
}

#[derive(Debug, Deserialize, Default)]
struct AxeResults {
    #[serde(default)]
    violations: Vec<AxeViolation>,
}

#[derive(Debug, Deserialize, Default)]
struct AxeViolation {
    #[serde(default)]
    id: String,
    #[serde(default)]
    impact: Option<String>,
    #[serde(default)]
    nodes: Vec<AxeNode>,
}

#[derive(Debug, Deserialize, Default)]
struct AxeNode {
    #[serde(default)]
    target: Vec<String>,
}

fn impact_severity(impact: Option<&str>) -> Severity {
    match impact.map(str::to_ascii_lowercase).as_deref() {
        Some("critical" | "serious") => Severity::Error,
        Some("moderate") => Severity::Warn,
        _ => Severity::Info,
    }
}

fn bound_token(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return String::new();
    }
    if collapsed.len() > MAX_A11Y_IMPORT_TOKEN {
        collapsed[..MAX_A11Y_IMPORT_TOKEN].to_owned()
    } else {
        collapsed
    }
}

fn finding(
    snapshot: &LayoutSnapshot,
    producer: &str,
    rule: &str,
    target: &str,
    severity: Severity,
    node_count: u32,
) -> UiIntegrityFinding {
    UiIntegrityFinding {
        check: UiCheck::ImportedA11y,
        severity,
        state: snapshot.state_key(),
        route: snapshot.route.clone(),
        viewport: snapshot.viewport.label(),
        subject: format!("axe:{rule}:{target}"),
        counterpart: None,
        component_hint: None,
        nodes: Vec::new(),
        evidence: UiEvidence {
            duplicate_count: node_count,
            ..UiEvidence::default()
        },
        detail: format!(
            "{producer} rule `{rule}` failed on `{target}` (impact-mapped severity, {node_count} node(s))"
        ),
    }
}
