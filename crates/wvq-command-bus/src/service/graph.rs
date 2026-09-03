//! Weavatrix graph helpers used by recovery, protection, and authoring.

use std::path::Path;

use serde_json::{Value, json};
use wvq_domain::RevisionId;
use wvq_intelligence::{CodeEvidenceProvider, WeavatrixProvider};

use super::BusError;
use super::paths::is_test_path;

pub(in crate::service) fn protection_graph_for_files(
    repo: &Path,
    revision: &RevisionId,
    files: &[String],
) -> Result<Value, BusError> {
    graph_for_files(repo, revision, files, false)
}

/// Same bounded Weavatrix neighbourhood, including test nodes.
///
/// Mutation ownership walks directed test → production reach. query_graph hides
/// `*_test.go` / `*.test.*` unless `include_tests` is set.
pub(in crate::service) fn mutation_reach_graph(
    repo: &Path,
    revision: &RevisionId,
    files: &[String],
) -> Result<Value, BusError> {
    graph_for_files(repo, revision, files, true)
}

fn graph_for_files(
    repo: &Path,
    revision: &RevisionId,
    files: &[String],
    include_tests: bool,
) -> Result<Value, BusError> {
    let indexed = WeavatrixProvider
        .indexed_files(repo)
        .map_err(|err| BusError::Intelligence(err.to_string()))?;
    let seeds = files
        .iter()
        .filter(|path| repo.join(path).is_file() && indexed.contains(path.as_str()))
        .map(|path| format!("file:{path}"))
        .collect::<Vec<_>>();
    if seeds.is_empty() {
        return Ok(json!({"nodes": [], "edges": [], "revision": revision.as_str()}));
    }
    let report = WeavatrixProvider
        .operation(
            repo,
            "query_graph",
            &json!({
                "seed_files": seeds,
                "depth": 8,
                "max_nodes": 100_000,
                "flow_direction": "both",
                "mode": "bfs",
                "include_tests": include_tests
            }),
        )
        .map_err(|err| BusError::Intelligence(err.to_string()))?;
    let found = report
        .get("revision")
        .and_then(Value::as_str)
        .ok_or_else(|| BusError::Intelligence("query_graph omitted revision identity".into()))?;
    if found != revision.as_str() {
        return Err(BusError::Ambiguous(format!(
            "query_graph evidence belongs to revision `{found}`, expected `{revision}`"
        )));
    }
    if report
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(BusError::Intelligence(
            "protection graph exceeded its bounded query; refusing partial coverage mapping".into(),
        ));
    }
    let mut nodes = report
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("node").cloned())
        .collect::<Vec<_>>();
    nodes.sort_by_key(graph_node_id);
    nodes.dedup_by(|left, right| graph_node_id(left) == graph_node_id(right));
    Ok(json!({
        "nodes": nodes,
        "edges": report.get("edges").cloned().unwrap_or_else(|| json!([])),
        "revision": revision.as_str()
    }))
}

pub(in crate::service) fn values_at<'a>(value: &'a Value, pointer: &str) -> &'a [Value] {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

pub(in crate::service) fn graph_node_id(node: &Value) -> Option<String> {
    node.get("id")
        .or_else(|| node.get("label"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

pub(in crate::service) fn graph_node_is_public_function(node: &Value) -> bool {
    let kind = node
        .get("kind")
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase);
    matches!(kind.as_deref(), Some("function" | "method"))
        && node
            .pointer("/attributes/exported")
            .and_then(Value::as_bool)
            == Some(true)
        && node
            .pointer("/attributes/test_only")
            .and_then(Value::as_bool)
            != Some(true)
        && graph_node_source_path(node).is_none_or(|path| !is_test_path(path))
}

pub(in crate::service) fn graph_node_source_path(node: &Value) -> Option<&str> {
    node.get("path").and_then(Value::as_str).or_else(|| {
        node.get("id")
            .and_then(Value::as_str)?
            .strip_prefix("symbol:")?
            .split_once('#')
            .map(|(path, _)| path)
    })
}

pub(in crate::service) fn recovery_public_symbol_id(node: &Value) -> Option<String> {
    let id = graph_node_id(node)?;
    Some(
        id.rsplit_once('@')
            .map_or(id.clone(), |(stable, _)| stable.to_owned()),
    )
}

pub(in crate::service) fn surface_labels(nodes: &[Value]) -> Vec<String> {
    nodes
        .iter()
        .filter(|node| {
            node.get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| {
                    ["endpoint", "route", "contract", "event"]
                        .iter()
                        .any(|surface| kind.contains(surface))
                })
        })
        .filter_map(|node| {
            node.get("label")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect()
}
