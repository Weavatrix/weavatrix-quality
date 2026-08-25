//! Extracted command-bus helper.

use super::access::*;
use super::selection_audit::read_single_run_json;
use super::verify_json::{json_string, json_u64, mandatory_test_paths, parse_axis_state, parse_ui_findings};

pub(in crate::service) fn protection_axis_from(
    deltas: &[ProtectionDelta],
    findings: &[ProtectionFinding],
) -> ProtectionAxis {
    let mut lost_flows = Vec::new();
    let mut lost_critical_branches = Vec::new();
    for delta in deltas {
        if matches!(delta.state, ProtectionDeltaState::Lost) {
            lost_flows.push(delta.flow.clone());
        }
        lost_critical_branches.extend(delta.lost_critical_branches.iter().cloned());
    }
    lost_flows.sort();
    lost_flows.dedup();
    lost_critical_branches.sort();
    lost_critical_branches.dedup();
    let (blocking_findings, warning_findings): (Vec<_>, Vec<_>) = findings
        .iter()
        .filter(|finding| finding.severity != Severity::Info)
        .cloned()
        .partition(|finding| finding.severity == Severity::Error);
    let state = if !blocking_findings.is_empty() || !lost_critical_branches.is_empty() {
        AxisState::Blocking
    } else if warning_findings.is_empty() {
        AxisState::Clean
    } else {
        AxisState::Warnings
    };
    ProtectionAxis {
        state,
        measured: true,
        summary: summarise(deltas),
        lost_flows,
        lost_critical_branches,
        blocking_findings,
        warning_findings,
    }
}

/// Project the debt ratchet onto the verdict axis.
///
/// Existing debt is counted and never blocks adoption; only findings this
/// change introduced or brought back are classified by rule family.
pub(in crate::service) fn debt_axis_from(reply: &DebtReply) -> DebtAxis {
    let mut new = Vec::new();
    let mut returned = Vec::new();
    for summary in &reply.findings {
        let Some((bucket, rest)) = summary.split_once(": ") else {
            continue;
        };
        let (id, rule) = match rest.split_once(" (") {
            Some((id, rule)) => (id, rule.trim_end_matches(')')),
            None => (rest, ""),
        };
        let item = DebtItem {
            id: id.to_owned(),
            rule: rule.to_owned(),
            blocking: debt_rule_blocks(rule),
        };
        match bucket {
            "new" => new.push(item),
            "returned" => returned.push(item),
            _ => {}
        }
    }
    new.sort_by(|left, right| left.id.cmp(&right.id));
    returned.sort_by(|left, right| left.id.cmp(&right.id));
    let blocking = new.iter().chain(&returned).any(|item| item.blocking);
    let state = if !reply.comparison_present {
        AxisState::Unmeasured
    } else if blocking {
        AxisState::Blocking
    } else if new.is_empty() && returned.is_empty() {
        AxisState::Clean
    } else {
        AxisState::Warnings
    };
    DebtAxis {
        state,
        comparison_present: reply.comparison_present,
        existing: reply.existing,
        fixed: reply.fixed,
        excepted: reply.excepted,
        new,
        returned,
    }
}

/// Test stability from the run's persisted analytics.
///
/// A mandatory flake is only escalated when deterministic triage could not
/// classify a *first* occurrence of a failure on a test bound to a high or
/// critical obligation. A known, already-clustered flake stays a warning.
pub(in crate::service) fn stability_axis(
    repo: &Path,
    store: &Store,
    run: &StoredRun,
    compiled: &Compiled,
) -> (StabilityAxis, Vec<Limitation>) {
    let Ok(analytics) = read_single_run_json(store, &run.id, "test-analytics") else {
        return (StabilityAxis::default(), Vec::new());
    };
    let flaky = values_at(&analytics, "/flaky_tests");
    let occurrences = values_at(&analytics, "/failure_occurrences");
    let mandatory_paths = mandatory_test_paths(repo, compiled);
    let mut unresolved = Vec::new();
    let mut unknown_failures = 0_u64;
    for occurrence in occurrences {
        if occurrence.get("classification").and_then(Value::as_str) != Some("unknown") {
            continue;
        }
        unknown_failures = unknown_failures.saturating_add(1);
        let first_seen = occurrence
            .get("previous_occurrences")
            .and_then(Value::as_u64)
            .is_some_and(|count| count == 0);
        let suite = occurrence
            .get("suite")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let name = occurrence
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if first_seen && mandatory_paths.contains(&normalize_path(suite)) {
            unresolved.push(format!("{suite}::{name}"));
        }
    }
    unresolved.sort();
    unresolved.dedup();
    let flaky_count = u64::try_from(flaky.len()).unwrap_or(u64::MAX);
    let state = if !unresolved.is_empty() {
        AxisState::Blocking
    } else if flaky_count > 0 || unknown_failures > 0 {
        AxisState::Warnings
    } else {
        AxisState::Clean
    };
    (
        StabilityAxis {
            state,
            measured: true,
            flaky: flaky_count,
            unknown_failures,
            unresolved_mandatory_flakes: unresolved,
        },
        Vec::new(),
    )
}

/// Deterministic UI integrity from the run's persisted ratchet.
///
/// The collector is the only thing that knows whether a route/state/viewport
/// was reachable, so `unmeasured` is reported by the producer rather than
/// guessed here. With no artifact at all this change has no UI surface and the
/// axis is `not_applicable`.
pub(in crate::service) fn ui_integrity_axis(
    store: &Store,
    run: &StoredRun,
    _compiled: &Compiled,
) -> Result<(UiIntegrityAxis, Vec<Limitation>), BusError> {
    let Ok(document) = read_single_run_json(store, &run.id, UI_INTEGRITY_DELTA_KIND) else {
        return Ok((UiIntegrityAxis::default(), Vec::new()));
    };
    if document.get("schema_v").and_then(Value::as_u64) != Some(1) {
        return Err(BusError::Store(
            "unknown ui-integrity-delta schema version".into(),
        ));
    }
    let axis = UiIntegrityAxis {
        state: parse_axis_state(document.get("state").and_then(Value::as_str))?,
        new: parse_ui_findings(&document, "new")?,
        returned: parse_ui_findings(&document, "returned")?,
        existing: json_u64(&document, "existing"),
        fixed: json_u64(&document, "fixed"),
        excepted: json_u64(&document, "excepted"),
        unmeasured_states: values_at(&document, "/unmeasured_states")
            .iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect(),
        truncated: document
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || document
                .get("responsive_truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false),
    };
    Ok((axis, Vec::new()))
}

pub(in crate::service) fn delta_triangle_axis(
    store: &Store,
    run: &StoredRun,
) -> Result<(DeltaTriangleAxis, Vec<Limitation>), BusError> {
    let Ok(document) = read_single_run_json(store, &run.id, DELTA_TRIANGLE_KIND) else {
        return Ok((DeltaTriangleAxis::default(), Vec::new()));
    };
    if !matches!(
        document.get("schema_v").and_then(Value::as_u64),
        Some(1..=3)
    ) {
        return Err(BusError::Store(
            "unknown delta-triangle schema version".into(),
        ));
    }
    let mut unmeasured_programs = values_at(&document, "/unmeasured_programs")
        .iter()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect::<Vec<_>>();
    unmeasured_programs.extend(
        values_at(&document, "/code_unmeasured_programs")
            .iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned)),
    );
    unmeasured_programs.sort();
    unmeasured_programs.dedup();
    let mut findings = Vec::new();
    for value in values_at(&document, "/findings") {
        let severity = match value.get("severity").and_then(Value::as_str) {
            Some("info") => Severity::Info,
            Some("warn") => Severity::Warn,
            Some("error") => Severity::Error,
            other => {
                return Err(BusError::Store(format!(
                    "delta-triangle finding has unknown severity `{}`",
                    other.unwrap_or("<missing>")
                )));
            }
        };
        findings.push(DeltaFindingRef {
            check: json_string(value, "check")?,
            severity,
            program: json_string(value, "program")?,
            detail: json_string(value, "detail")?,
        });
    }
    let axis = DeltaTriangleAxis {
        state: parse_axis_state(document.get("state").and_then(Value::as_str))?,
        spec_changed: document
            .get("spec_changed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        code_changed: document
            .get("code_changed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        behavior_changed: document
            .get("behavior_changed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        measured_programs: json_u64(&document, "measured_programs"),
        changed_programs: values_at(&document, "/changed_programs")
            .iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect(),
        readings: values_at(&document, "/readings")
            .iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect(),
        findings,
        unmeasured_programs: unmeasured_programs.clone(),
    };
    let replay_detail = document
        .get("replay_limitation")
        .and_then(Value::as_str)
        .filter(|detail| !detail.is_empty());
    let limitations = (!unmeasured_programs.is_empty())
        .then(|| Limitation {
            axis: "delta_triangle".into(),
            detail: format!(
                "same-program base/head replay was incomplete for {}{}",
                unmeasured_programs.join(", "),
                replay_detail.map_or_else(String::new, |detail| format!(": {detail}"))
            ),
        })
        .into_iter()
        .collect();
    Ok((axis, limitations))
}

