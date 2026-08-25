//! Extracted command-bus helper.

use super::access::*;

use super::persist_browser::{MAX_UI_ARTIFACT_BYTES, MAX_UI_REPLY_FINDINGS};
use super::persist_run::put_run_artifact;

pub(in crate::service) struct CollectedUi {
    pub(in crate::service) snapshot: UiIntegritySnapshot,
    pub(in crate::service) layouts: Value,
    pub(in crate::service) hit_test_map: Value,
}

/// Decode, validate, and analyse every layout snapshot one run collected.
///
/// A snapshot the collector could not take, or one it flagged as unsettled or
/// truncated, marks the whole measurement incomplete. That is deliberately
/// louder than dropping it: an unmeasured state must not read as a clean one.
pub(in crate::service) fn analyse_ui_snapshots(
    revision: &RevisionId,
    policy: &UiIntegrityPolicy,
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
) -> Result<CollectedUi, BusError> {
    let mut snapshot = UiIntegritySnapshot {
        revision: revision.to_string(),
        ..UiIntegritySnapshot::default()
    };
    let mut layouts = Vec::new();
    let mut hit_map = Vec::new();
    for (configured, result) in browser_runs {
        let duplicate_mutations = wvq_runtime::duplicate_mutation_requests(result);
        if result
            .observations
            .iter()
            .any(|observation| observation.network_requests_truncated)
            || !matches!(
                configured.program.evidence_policy.network,
                CaptureWhen::Always
            )
        {
            snapshot.truncated = true;
        }
        for evidence in &result.ui_snapshots {
            if !evidence.limitations.is_empty() || evidence.snapshot.is_null() {
                snapshot.truncated = true;
            }
            if evidence.snapshot.is_null() {
                continue;
            }
            let layout: LayoutSnapshot = serde_json::from_value(evidence.snapshot.clone())
                .map_err(|err| {
                    BusError::Runtime(format!(
                        "browser returned a malformed layout snapshot for {} step {}: {err}",
                        result.program, evidence.step
                    ))
                })?;
            if layout.revision.as_str() != revision.as_str() {
                return Err(BusError::Ambiguous(format!(
                    "layout snapshot for {} claims revision `{}`, the run is at `{revision}`",
                    result.program, layout.revision
                )));
            }
            let output = detect_ui(&layout, policy)
                .map_err(|err| BusError::Runtime(format!("ui integrity: {err}")))?;
            snapshot.truncated |= output.truncated;
            snapshot
                .responsive_breakpoints
                .extend(layout.responsive_breakpoints.iter().copied());
            snapshot.responsive_breakpoints_incomplete |= !layout.responsive_breakpoints_complete;
            snapshot.measured_states.insert(layout.state_key());
            snapshot.findings.extend(output.findings);
            if let Some(report) = &evidence.a11y_import {
                match wvq_ui::import_a11y_violations(&layout, report) {
                    Ok((imported, truncated)) => {
                        snapshot.truncated |= truncated;
                        snapshot.findings.extend(imported);
                    }
                    Err(_) => snapshot.truncated = true,
                }
            }
            snapshot.findings.extend(
                duplicate_mutations
                    .iter()
                    .filter(|duplicate| duplicate.step == evidence.step)
                    .map(|duplicate| duplicate_mutation_finding(&layout, duplicate)),
            );
            hit_map.push(hit_test_summary(&layout));
            layouts.push(serde_json::to_value(&layout).map_err(|err| {
                BusError::Runtime(format!("cannot encode layout snapshot: {err}"))
            })?);
        }
    }
    wvq_ui::sort_findings(&mut snapshot.findings);
    Ok(CollectedUi {
        layouts: json!({
            "schema_v": 1,
            "revision": revision.as_str(),
            "snapshots": layouts,
        }),
        hit_test_map: json!({
            "schema_v": 1,
            "revision": revision.as_str(),
            "targets": hit_map,
        }),
        snapshot,
    })
}

pub(in crate::service) fn duplicate_mutation_finding(
    layout: &LayoutSnapshot,
    duplicate: &wvq_runtime::DuplicateMutationRequest,
) -> UiIntegrityFinding {
    let count = u32::try_from(duplicate.sequences.len()).unwrap_or(u32::MAX);
    UiIntegrityFinding {
        check: wvq_ui::UiCheck::DuplicateMutationRequest,
        severity: Severity::Error,
        state: layout.state_key(),
        route: layout.route.clone(),
        viewport: format!("{}x{}", layout.viewport.width, layout.viewport.height),
        subject: format!("{} {}", duplicate.method, duplicate.url),
        counterpart: None,
        component_hint: None,
        nodes: Vec::new(),
        evidence: wvq_ui::UiEvidence {
            duplicate_count: count,
            ..wvq_ui::UiEvidence::default()
        },
        detail: format!(
            "one action at step {} emitted the same mutating request {count} times (request sequences {})",
            duplicate.step,
            duplicate
                .sequences
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Per-target hit-test totals: what was probed, what got through, and what
/// intercepted the rest. Small enough to read, exact enough to act on.
pub(in crate::service) fn hit_test_summary(layout: &LayoutSnapshot) -> Value {
    let index = layout.index();
    let mut targets: BTreeMap<String, (u32, u32, BTreeMap<String, u32>)> = BTreeMap::new();
    for sample in &layout.hit_tests {
        let entry = targets
            .entry(sample.target.to_string())
            .or_insert_with(|| (0, 0, BTreeMap::new()));
        entry.0 += 1;
        match &sample.topmost {
            Some(topmost) if index.is_self_or_descendant(topmost, &sample.target) => entry.1 += 1,
            Some(topmost) => {
                *entry
                    .2
                    .entry(
                        index
                            .node(topmost)
                            .map_or_else(|| topmost.to_string(), wvq_ui::UiNode::semantic_identity),
                    )
                    .or_default() += 1;
            }
            None => entry.1 += 1,
        }
    }
    json!({
        "state": layout.state_key(),
        "route": layout.route,
        "viewport": layout.viewport.label(),
        "targets": targets
            .into_iter()
            .map(|(target, (samples, received, blockers))| {
                json!({
                    "target": index
                        .node(&wvq_ui::UiNodeId::new(&target).unwrap_or_default())
                        .map_or(target.clone(), wvq_ui::UiNode::semantic_identity),
                    "node": target,
                    "samples": samples,
                    "received_events": received,
                    "blockers": blockers,
                })
            })
            .collect::<Vec<_>>(),
    })
}

pub(in crate::service) fn put_bounded_ui_artifact(
    store: &Store,
    run_id: &RunId,
    raw_id: &str,
    kind: &str,
    value: &Value,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|err| BusError::Runtime(format!("cannot encode {kind}: {err}")))?;
    if bytes.len() > MAX_UI_ARTIFACT_BYTES {
        // Refuse rather than store a partial document: half a snapshot would be
        // analysed as though it were the whole page.
        return Err(BusError::Runtime(format!(
            "{kind} is {} bytes, over the {MAX_UI_ARTIFACT_BYTES}-byte ceiling; \
             lower ui_integrity.max_nodes",
            bytes.len()
        )));
    }
    put_run_artifact(store, run_id, raw_id, kind, &bytes, handles)
}
