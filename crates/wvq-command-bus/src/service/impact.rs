//! Extracted command-bus helper.

use super::access::*;
use super::protection_snapshot::ensure_complete_diff;

pub(in crate::service) fn merge_browser_proof_evidence(
    evidence: &mut BTreeMap<String, BrowserProofEvidence>,
    bytes: &[u8],
) -> Result<(), BusError> {
    let stored: StoredBrowserProgramEvidence = serde_json::from_slice(bytes)
        .map_err(|err| BusError::Store(format!("invalid browser program evidence: {err}")))?;
    if stored.schema_v != 2 {
        return Err(BusError::Store(format!(
            "unknown browser program evidence schema {}",
            stored.schema_v
        )));
    }
    if stored.program.is_empty() {
        return Err(BusError::Store(
            "browser program evidence omitted program identity".into(),
        ));
    }
    let expected_asserted = stored
        .assertions
        .iter()
        .filter(|assertion| assertion.status == "passed")
        .map(|assertion| assertion.obligation.clone())
        .collect::<BTreeSet<_>>();
    let expected_contradicted = stored
        .assertions
        .iter()
        .filter(|assertion| assertion.status == "contradicted")
        .map(|assertion| assertion.obligation.clone())
        .collect::<BTreeSet<_>>();
    if expected_asserted != stored.asserted.iter().cloned().collect()
        || expected_contradicted != stored.contradicted.iter().cloned().collect()
    {
        return Err(BusError::Store(format!(
            "browser program {} aggregate assertion lists do not match exact evidence",
            stored.program
        )));
    }
    for assertion in &stored.assertions {
        if assertion.obligation.is_empty()
            || !matches!(
                assertion.status.as_str(),
                "passed" | "contradicted" | "failed"
            )
            || assertion
                .observation
                .as_ref()
                .is_some_and(|observation| !stored.observations.contains(observation))
        {
            return Err(BusError::Store(format!(
                "browser program {} has invalid exact assertion evidence",
                stored.program
            )));
        }
        let entry = evidence.entry(assertion.obligation.clone()).or_default();
        entry.programs.insert(stored.program.clone());
        for kind in &stored.present {
            if !entry.present.contains(kind) {
                entry.present.push(*kind);
            }
        }
        if let Some(observation) = &assertion.observation
            && !entry.observations.contains(observation)
        {
            entry.observations.push(observation.clone());
        }
        match assertion.status.as_str() {
            "passed" => entry.passed = true,
            "failed" => entry.failed = true,
            "contradicted" => entry.contradicted = true,
            _ => unreachable!("validated browser assertion status"),
        }
    }
    for observation in &stored.observations {
        if observation.is_empty() {
            return Err(BusError::Store(format!(
                "browser program {} has an empty observation handle",
                stored.program
            )));
        }
    }
    Ok(())
}

pub(in crate::service) fn live_impacted_surface(
    diff: &Value,
    impact: &Value,
) -> Result<wvq_intelligence::ImpactedSurface, BusError> {
    ensure_complete_diff(diff)?;
    let mut base = Vec::new();
    let mut head = Vec::new();
    let mut removed_nodes = Vec::new();
    let mut removed_edges = Vec::new();

    for node in values_at(diff, "/nodes/removed") {
        if let Some(id) = graph_node_id(node) {
            base.push(id.clone());
            removed_nodes.push(id);
        }
    }
    for node in values_at(diff, "/nodes/added") {
        if let Some(id) = graph_node_id(node) {
            head.push(id);
        }
    }
    for changed in values_at(diff, "/nodes/changed") {
        if let Some(id) = changed.get("before").and_then(graph_node_id) {
            base.push(id);
        }
        if let Some(id) = changed.get("after").and_then(graph_node_id) {
            head.push(id);
        }
    }
    for edge in values_at(diff, "/edges/removed") {
        let source = edge
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let target = edge
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !source.is_empty() {
            base.push(source.to_owned());
        }
        if !target.is_empty() {
            base.push(target.to_owned());
        }
        removed_edges.push(format!("{source}->{target}"));
    }
    for edge in values_at(diff, "/edges/added") {
        if let Some(source) = edge.get("source").and_then(Value::as_str) {
            head.push(source.to_owned());
        }
        if let Some(target) = edge.get("target").and_then(Value::as_str) {
            head.push(target.to_owned());
        }
    }
    for node in values_at(impact, "/impacted_nodes") {
        if let Some(id) = graph_node_id(node) {
            head.push(id);
        }
    }

    let surfaces = SurfaceDelta {
        added: surface_labels(values_at(diff, "/nodes/added")),
        removed: surface_labels(values_at(diff, "/nodes/removed")),
    };
    Ok(impacted_surface(
        &base,
        &head,
        &GraphDelta {
            removed_nodes,
            removed_edges,
        },
        &surfaces,
    ))
}

