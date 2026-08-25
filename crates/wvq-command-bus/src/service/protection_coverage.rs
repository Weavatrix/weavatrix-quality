//! Extracted command-bus helper.

use super::access::*;

use super::persist_run::normalized_suite_matches;

pub(in crate::service) fn measured_protection_flows(
    repo: &Path,
    revision: &RevisionId,
    graph: &Value,
    nodes: &[Value],
    records: &[ExecutorRecord],
    bindings: &[TestBinding],
) -> Result<(BTreeMap<String, FlowProtection>, BTreeSet<String>), BusError> {
    let mut known_nodes = BTreeSet::new();
    let symbol_files = nodes
        .iter()
        .filter(|node| node.get("kind").and_then(Value::as_str) != Some("file"))
        .filter_map(|node| node.pointer("/span/file").and_then(Value::as_str))
        .map(normalize_path)
        .collect::<BTreeSet<_>>();
    for node in nodes {
        let Some(id) = graph_node_id(node) else {
            return Err(BusError::Intelligence(
                "impacted head graph contains a node without identity".into(),
            ));
        };
        known_nodes.insert(id);
    }

    let mut flows = BTreeMap::<String, FlowProtection>::new();
    let mut coverage_files = BTreeSet::new();
    for record in records.iter().filter(|record| record.passed) {
        let protectors = coverage_protectors(repo, record, bindings)?;
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
            coverage_files.extend(coverage.files.iter().map(|file| file.path.clone()));
            let mapped = map_coverage_to_nodes(Some(&coverage), graph)
                .map_err(|err| BusError::Intelligence(err.to_string()))?;
            for node in mapped
                .into_iter()
                .filter(|node| node.measurement != CoverageMeasurement::Unmeasured)
            {
                let node_id = node.node_id;
                if node_id
                    .strip_prefix("file:")
                    .is_some_and(|path| symbol_files.contains(&normalize_path(path)))
                {
                    // A file node is only a fallback when Weavatrix has no
                    // symbol span for that source. Keeping both lets one hit in
                    // ViewerLabel hide an uncovered CanDelete function.
                    continue;
                }
                if !known_nodes.contains(&node_id) {
                    return Err(BusError::Intelligence(format!(
                        "coverage mapped unknown graph node {node_id}"
                    )));
                }
                let flow = flows
                    .entry(node_id.clone())
                    .or_insert_with(|| FlowProtection {
                        flow: node_id.clone(),
                        revision: revision.to_string(),
                        tests: Vec::new(),
                        sessions: Vec::new(),
                        covered_nodes: Vec::new(),
                        covered_branches: Vec::new(),
                        proven_obligations: Vec::new(),
                        proofs: Vec::new(),
                    });
                if node.measurement != CoverageMeasurement::Covered {
                    continue;
                }
                for protector in &protectors {
                    if !flow.tests.contains(&protector.identity) {
                        flow.tests.push(protector.identity.clone());
                    }
                    for obligation in &protector.obligations {
                        if !flow.proven_obligations.contains(obligation) {
                            flow.proven_obligations.push(obligation.clone());
                        }
                    }
                }
                if !flow.covered_nodes.contains(&node_id) {
                    flow.covered_nodes.push(node_id);
                }
            }
        }
    }
    Ok((flows, coverage_files))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::service) struct CoverageProtector {
    pub(in crate::service) identity: String,
    pub(in crate::service) obligations: Vec<String>,
}

/// Attribute one coverage artifact to an exact test only when the invocation
/// makes that attribution unambiguous. A batch-wide coverage file remains
/// executor-level evidence: guessing which case reached a flow would turn a
/// test list into proof.
pub(in crate::service) fn coverage_protectors(
    repo: &Path,
    record: &ExecutorRecord,
    bindings: &[TestBinding],
) -> Result<Vec<CoverageProtector>, BusError> {
    let mut cases = Vec::new();
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
        cases.extend(
            normalized
                .cases
                .into_iter()
                .filter(|case| case.status == TestStatus::Pass),
        );
    }
    if cases.len() == 1 {
        let case = &cases[0];
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
            .collect::<Vec<_>>();
        if !matched.is_empty() {
            let mut by_identity = BTreeMap::<String, BTreeSet<String>>::new();
            for binding in matched {
                by_identity
                    .entry(format!("{}#{}", binding.path, case.name))
                    .or_default()
                    .extend(binding.obligations.iter().cloned());
            }
            return Ok(by_identity
                .into_iter()
                .map(|(identity, obligations)| CoverageProtector {
                    identity,
                    obligations: obligations.into_iter().collect(),
                })
                .collect());
        }
        return Ok(vec![CoverageProtector {
            identity: format!("{}:{}#{}", record.executor, case.suite, case.name),
            obligations: Vec::new(),
        }]);
    }
    if record.selection.len() == 1 {
        let identity = record.selection[0].clone();
        let obligations = bindings
            .iter()
            .filter(|binding| binding.case.is_none() && binding.path == identity)
            .flat_map(|binding| binding.obligations.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        return Ok(vec![CoverageProtector {
            identity,
            obligations,
        }]);
    }
    Ok(vec![CoverageProtector {
        identity: format!("executor:{}@{}", record.executor, record.cwd),
        obligations: Vec::new(),
    }])
}

pub(in crate::service) fn coverage_graph_mismatch(nodes: &[Value], coverage_files: BTreeSet<String>) -> BusError {
    let graph_files = nodes
        .iter()
        .filter_map(|node| {
            node.pointer("/span/file")
                .or_else(|| node.get("file"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect::<BTreeSet<_>>();
    let graph_spans = nodes
        .iter()
        .filter_map(|node| {
            Some(format!(
                "{}:{}-{}",
                node.pointer("/span/file")
                    .or_else(|| node.get("file"))?
                    .as_str()?,
                node.pointer("/span/start_line")
                    .or_else(|| node.pointer("/span/start/line"))
                    .or_else(|| node.get("start_line"))?
                    .as_u64()?,
                node.pointer("/span/end_line")
                    .or_else(|| node.pointer("/span/end/line"))
                    .or_else(|| node.get("end_line"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            ))
        })
        .take(20)
        .collect::<Vec<_>>();
    let graph_sample = nodes.iter().take(3).cloned().collect::<Vec<_>>();
    BusError::Intelligence(format!(
        "coverage files [{}] do not overlap measured protection spans [{}] in graph files [{}]; graph sample {}",
        coverage_files.into_iter().collect::<Vec<_>>().join(", "),
        graph_spans.join(", "),
        graph_files.into_iter().collect::<Vec<_>>().join(", "),
        Value::Array(graph_sample)
    ))
}
