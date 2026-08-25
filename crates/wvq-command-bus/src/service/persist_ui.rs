//! Extracted command-bus helper.

//! Extracted command-bus helper.

use super::access::*;
use super::persist_browser::MAX_UI_REPLY_FINDINGS;
use super::persist_ui_analyse::{analyse_ui_snapshots, put_bounded_ui_artifact};

pub(in crate::service) fn ui_delta_document(delta: &UiIntegrityDelta) -> Value {
    let state = if delta.blocks() {
        "blocking"
    } else if delta.truncated || delta.responsive_truncated || !delta.unmeasured_states.is_empty() {
        "unmeasured"
    } else if delta.new.is_empty() && delta.returned.is_empty() && delta.existing.is_empty() {
        "clean"
    } else if delta.new.is_empty() && delta.returned.is_empty() {
        // Only pre-existing debt survived: recorded, not held against the change.
        "clean"
    } else {
        "warnings"
    };
    json!({
        "schema_v": 1,
        "state": state,
        "new": ui_finding_refs_with_intervals(&delta.new, &delta.responsive_intervals, UiFindingState::New),
        "returned": ui_finding_refs_with_intervals(&delta.returned, &delta.responsive_intervals, UiFindingState::Returned),
        "existing": delta.existing.len(),
        "fixed": delta.fixed.len(),
        "excepted": delta.excepted.len(),
        "unmeasured_states": delta.unmeasured_states,
        "truncated": delta.truncated,
        "expired_policy": delta.expired_policy,
        "responsive_intervals": delta.responsive_intervals,
        "responsive_truncated": delta.responsive_truncated,
        "runtime_llm_tokens": 0,
    })
}

pub(in crate::service) fn responsive_probe_incomplete(probe: &ResponsiveProbe) -> bool {
    probe.delta.truncated || !probe.delta.unmeasured_states.is_empty()
}

pub(in crate::service) fn ui_finding_refs_with_intervals(
    findings: &[UiIntegrityFinding],
    intervals: &[wvq_ui::ResponsiveFailureInterval],
    state: UiFindingState,
) -> Vec<Value> {
    let mut out = ui_finding_refs(findings);
    out.extend(
        intervals
            .iter()
            .filter(|interval| interval.state == state)
            .take(MAX_UI_REPLY_FINDINGS.saturating_sub(out.len()))
            .map(|interval| {
                let height = interval
                    .finding
                    .viewport
                    .split_once('x')
                    .map_or("?", |(_, height)| height);
                json!({
                    "check": interval.finding.check.id(),
                    "severity": match interval.finding.severity {
                        Severity::Info => "info",
                        Severity::Warn => "warn",
                        Severity::Error => "error",
                    },
                    "subject": interval.finding.subject,
                    "route": interval.finding.route,
                    "viewport": format!("{}-{}x{height}", interval.first_width, interval.last_width),
                    "detail": format!(
                        "{}; responsive failure interval {}..={} px (lower exact: {}, upper exact: {})",
                        interval.finding.detail,
                        interval.first_width,
                        interval.last_width,
                        interval.lower_boundary_exact,
                        interval.upper_boundary_exact,
                    ),
                })
            }),
    );
    out
}

pub(in crate::service) fn ui_finding_refs(findings: &[UiIntegrityFinding]) -> Vec<Value> {
    findings
        .iter()
        .take(MAX_UI_REPLY_FINDINGS)
        .map(|finding| {
            json!({
                "check": finding.check.id(),
                "severity": match finding.severity {
                    Severity::Info => "info",
                    Severity::Warn => "warn",
                    Severity::Error => "error",
                },
                "subject": finding.subject,
                "route": finding.route,
                "viewport": finding.viewport,
                "detail": finding.detail,
            })
        })
        .collect()
}

/// Turn one run's collected layout snapshots into stored UI-integrity evidence.
///
/// Three artifacts come out of this: the raw bounded snapshots, a compact
/// hit-test map for `quality_explain`, and the findings the detectors produced.
/// All three are CAS handles; none of them is ever inlined into an MCP reply.
///
/// Returns the head-side snapshot so a base/head comparison can use it.
pub(in crate::service) fn persist_ui_integrity(
    store: &Store,
    run_id: &RunId,
    revision: &RevisionId,
    policy: &UiIntegrityPolicy,
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
    handles: &mut Vec<String>,
) -> Result<Option<UiIntegritySnapshot>, BusError> {
    if !policy.enabled {
        return Ok(None);
    }
    let collected = analyse_ui_snapshots(revision, policy, browser_runs)?;
    if collected.snapshot.measured_states.is_empty() && !collected.snapshot.truncated {
        // No browser program produced a snapshot: this run has no UI surface.
        return Ok(None);
    }
    put_bounded_ui_artifact(
        store,
        run_id,
        &format!("artifact-{}-ui-layout-snapshot", run_id.as_str()),
        "ui-layout-snapshot",
        &collected.layouts,
        handles,
    )?;
    put_bounded_ui_artifact(
        store,
        run_id,
        &format!("artifact-{}-ui-hit-test-map", run_id.as_str()),
        "ui-hit-test-map",
        &collected.hit_test_map,
        handles,
    )?;
    put_bounded_ui_artifact(
        store,
        run_id,
        &format!("artifact-{}-ui-integrity-findings", run_id.as_str()),
        "ui-integrity-findings",
        &json!({
            "schema_v": 1,
            "revision": revision.as_str(),
            "measured_states": collected.snapshot.measured_states,
            "findings": collected.snapshot.findings,
            "responsive_breakpoints": collected.snapshot.responsive_breakpoints,
            "responsive_breakpoints_incomplete": collected.snapshot.responsive_breakpoints_incomplete,
            "truncated": collected.snapshot.truncated,
            "runtime_llm_tokens": 0,
        }),
        handles,
    )?;
    Ok(Some(collected.snapshot))
}

