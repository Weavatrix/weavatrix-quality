//! Extracted command-bus helper.

use super::access::*;
use super::persist_run::normalized_suite_matches;
use super::protection_coverage::{coverage_graph_mismatch, measured_protection_flows};

pub(in crate::service) fn ensure_complete_diff(diff: &Value) -> Result<(), BusError> {
    for (count, values) in [
        ("nodes_added", "/nodes/added"),
        ("nodes_removed", "/nodes/removed"),
        ("nodes_changed", "/nodes/changed"),
        ("edges_added", "/edges/added"),
        ("edges_removed", "/edges/removed"),
    ] {
        let expected = diff
            .pointer(&format!("/counts/{count}"))
            .and_then(Value::as_u64)
            .ok_or_else(|| BusError::Intelligence(format!("graph_diff omitted {count}")))?;
        let present = u64::try_from(values_at(diff, values).len()).unwrap_or(u64::MAX);
        if expected != present {
            return Err(BusError::Intelligence(format!(
                "graph_diff {count} is incomplete: expected {expected}, received {present}"
            )));
        }
    }
    Ok(())
}

pub(in crate::service) fn live_protection_snapshot(
    repo: &Path,
    revision: &RevisionId,
    graph: &Value,
    records: &[ExecutorRecord],
    bindings: &[TestBinding],
) -> Result<Option<wvq_proof::ProtectionSnapshot>, BusError> {
    let Some(nodes) = graph.get("nodes").and_then(Value::as_array) else {
        return Ok(None);
    };
    if nodes.is_empty() {
        return Ok(None);
    }
    let executed_tests = executed_test_inventory(repo, records, bindings)?;
    let (flows, coverage_files) =
        measured_protection_flows(repo, revision, graph, nodes, records, bindings)?;
    if flows.is_empty() {
        if !coverage_files.is_empty() {
            return Err(coverage_graph_mismatch(nodes, coverage_files));
        }
        return Ok(None);
    }
    let snapshot =
        snapshot_with_executed_tests(revision, flows.into_values().collect(), executed_tests)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
    Ok(Some(snapshot))
}

/// Record every exact passing case independently of the flows it covered.
///
/// Coverage attribution remains deliberately stricter: a batch artifact may
/// only protect at executor scope. The inventory has a different job — proving
/// that a named case still executed, even when it reached no impacted symbol.
pub(in crate::service) fn executed_test_inventory(
    repo: &Path,
    records: &[ExecutorRecord],
    bindings: &[TestBinding],
) -> Result<Vec<String>, BusError> {
    let mut identities = BTreeSet::new();
    for record in records.iter().filter(|record| record.passed) {
        for artifact in record
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == "normalized-test-run")
        {
            let normalized: NormalizedTestRun =
                serde_json::from_slice(&artifact.bytes).map_err(|err| {
                    BusError::Runtime(format!(
                        "cannot decode normalized evidence from {}: {err}",
                        artifact.path
                    ))
                })?;
            for case in normalized
                .cases
                .into_iter()
                .filter(|case| case.status == TestStatus::Pass)
            {
                let matched = bindings
                    .iter()
                    .filter(|binding| {
                        binding.case.as_deref() == Some(case.name.as_str())
                            && binding
                                .runner
                                .as_deref()
                                .is_none_or(|runner| runner == record.executor)
                            && normalized_suite_matches(repo, record, binding, &case.suite)
                    })
                    .map(|binding| format!("{}#{}", binding.path, case.name))
                    .collect::<Vec<_>>();
                if matched.is_empty() {
                    identities.insert(format!("{}:{}#{}", record.executor, case.suite, case.name));
                } else {
                    identities.extend(matched);
                }
            }
        }
    }
    Ok(identities.into_iter().collect())
}

pub(in crate::service) fn persist_dynamic_coverage_history(
    store: &Store,
    run: &RunId,
    revision: &RevisionId,
    graph: &Value,
    records: &[ExecutorRecord],
) -> Result<(), BusError> {
    let mut observations = BTreeMap::<String, BTreeSet<String>>::new();
    for record in records
        .iter()
        .filter(|record| record.passed && record.selection.len() == 1)
    {
        let test = &record.selection[0];
        if !is_test_path(test) {
            continue;
        }
        for artifact in record
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == "coverage")
        {
            let coverage: CoverageArtifact =
                serde_json::from_slice(&artifact.bytes).map_err(|err| {
                    BusError::Runtime(format!(
                        "cannot decode normalized coverage {}: {err}",
                        artifact.path
                    ))
                })?;
            let mapped = map_coverage_to_nodes(Some(&coverage), graph)
                .map_err(|err| BusError::Intelligence(err.to_string()))?;
            observations.entry(test.clone()).or_default().extend(
                mapped
                    .into_iter()
                    .filter(|node| node.measurement == CoverageMeasurement::Covered)
                    .map(|node| node.node_id),
            );
        }
    }
    for (test, nodes) in observations {
        store
            .observe_test_nodes(run, &test, &nodes.into_iter().collect::<Vec<_>>(), revision)
            .map_err(|err| BusError::Store(err.to_string()))?;
    }
    Ok(())
}

