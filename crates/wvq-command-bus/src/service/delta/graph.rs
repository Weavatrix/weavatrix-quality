//! Changed Weavatrix nodes and declared obligation-to-node flows.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use wvq_proof::FlowProtection;
use wvq_runtime::{AxisDelta, BehaviorDelta, DiffAxis, StructuredView, behavior_delta};

use super::super::{
    BusError, TestBinding, ensure_complete_diff, graph_node_id, graph_node_source_path,
    normalize_path, sha256_hex, values_at,
};

pub(in crate::service) fn graph_diff_changed_nodes(diff: &Value) -> Result<BTreeSet<String>, BusError> {
    ensure_complete_diff(diff)?;
    let mut nodes = BTreeSet::new();
    for node in values_at(diff, "/nodes/added") {
        if let Some(id) = graph_node_id(node) {
            nodes.insert(id);
        }
    }
    for node in values_at(diff, "/nodes/removed") {
        if let Some(id) = graph_node_id(node) {
            nodes.insert(id);
        }
    }
    for changed in values_at(diff, "/nodes/changed") {
        if let Some(id) = changed.get("before").and_then(graph_node_id) {
            nodes.insert(id);
        }
        if let Some(id) = changed.get("after").and_then(graph_node_id) {
            nodes.insert(id);
        }
        if let Some(id) = graph_node_id(changed) {
            nodes.insert(id);
        }
    }
    for edge in values_at(diff, "/edges/added")
        .iter()
        .chain(values_at(diff, "/edges/removed"))
    {
        if let Some(source) = edge.get("source").and_then(Value::as_str)
            && !source.is_empty()
        {
            nodes.insert(source.to_owned());
        }
        if let Some(target) = edge.get("target").and_then(Value::as_str)
            && !target.is_empty()
        {
            nodes.insert(target.to_owned());
        }
    }
    Ok(nodes)
}

pub(in crate::service) fn declared_code_flows(
    revision: &str,
    bindings: &[TestBinding],
    graph: &Value,
) -> Vec<FlowProtection> {
    let mut by_path = BTreeMap::<String, BTreeSet<String>>::new();
    for binding in bindings {
        by_path
            .entry(normalize_path(&binding.path))
            .or_default()
            .extend(binding.obligations.iter().cloned());
    }
    let mut flows = BTreeMap::<String, FlowProtection>::new();
    for node in values_at(graph, "/nodes") {
        let Some(file) = graph_node_file(node) else {
            continue;
        };
        let Some(obligations) = by_path.get(&file) else {
            continue;
        };
        let Some(node_id) = graph_node_id(node) else {
            continue;
        };
        let flow = flows
            .entry(node_id.clone())
            .or_insert_with(|| FlowProtection {
                flow: node_id.clone(),
                revision: revision.to_owned(),
                tests: Vec::new(),
                sessions: Vec::new(),
                covered_nodes: vec![node_id.clone()],
                covered_branches: Vec::new(),
                proven_obligations: Vec::new(),
                proofs: Vec::new(),
            });
        for obligation in obligations {
            if !flow.proven_obligations.contains(obligation) {
                flow.proven_obligations.push(obligation.clone());
            }
        }
        for binding in bindings
            .iter()
            .filter(|binding| normalize_path(&binding.path) == file)
        {
            if !flow.tests.contains(&binding.path) {
                flow.tests.push(binding.path.clone());
            }
        }
    }
    let mut flows = flows.into_values().collect::<Vec<_>>();
    for flow in &mut flows {
        flow.proven_obligations.sort();
        flow.proven_obligations.dedup();
        flow.tests.sort();
        flow.tests.dedup();
    }
    flows
}

fn graph_node_file(node: &Value) -> Option<String> {
    node.pointer("/span/file")
        .and_then(Value::as_str)
        .or_else(|| graph_node_source_path(node))
        .map(normalize_path)
}

pub(in crate::service::delta) fn paired_observation_delta(
    base: &[wvq_runtime::Observation],
    head: &[wvq_runtime::Observation],
) -> BehaviorDelta {
    let mut axes = BTreeMap::<DiffAxis, (Vec<String>, Vec<String>)>::new();
    let mut first_structured = None;
    for (index, (base, head)) in base.iter().zip(head).enumerate() {
        let delta = behavior_delta(
            &StructuredView::from_replay(base, None),
            &StructuredView::from_replay(head, None),
        );
        first_structured = first_structured.or(delta.first_structured);
        for axis in delta.axes {
            let entry = axes.entry(axis.axis).or_default();
            entry
                .0
                .push(format!("{index}:{}", sha256_hex(axis.base.as_bytes())));
            entry
                .1
                .push(format!("{index}:{}", sha256_hex(axis.head.as_bytes())));
        }
    }
    if first_structured.is_some() {
        axes.remove(&DiffAxis::VisualDigest);
    }
    let visual_compared = first_structured.is_none()
        && base.iter().zip(head).any(|(base_obs, head_obs)| {
            base_obs.visual_digest.is_some() && head_obs.visual_digest.is_some()
        });
    BehaviorDelta {
        axes: axes
            .into_iter()
            .map(|(axis, (base, head))| AxisDelta {
                axis,
                base: base.join(","),
                head: head.join(","),
            })
            .collect(),
        first_structured,
        visual_compared,
    }
}
