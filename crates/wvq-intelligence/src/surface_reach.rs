//! Reach from a declared binding path to production Weavatrix nodes.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_REACH_DEPTH: usize = 4;
const MAX_REACH_NODES: usize = 64;
const MAX_EVIDENCE_PATHS: usize = 32;

/// Production implementation nodes a binding may claim.
///
/// A production file binding returns the nodes in that file. A test/spec
/// binding follows **directed** graph edges (bounded) onto production source
/// nodes and never returns the test node itself. Reverse/undirected neighbours
/// are not production evidence.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingReach {
    /// Production nodes reached from the binding.
    pub nodes: Vec<String>,
    /// Directed paths that justified each node, bounded.
    pub evidence_paths: Vec<Vec<String>>,
    /// True when depth or node ceilings stopped the walk.
    pub truncated: bool,
}

/// Production implementation nodes a binding may claim.
#[must_use]
pub fn production_nodes_for_binding(graph: &Value, binding_path: &str) -> BindingReach {
    let path = normalize_path(binding_path);
    if path.is_empty() {
        return BindingReach::default();
    }
    let mut file_of = BTreeMap::<String, String>::new();
    let mut on_path = Vec::new();
    for node in values(graph, "nodes") {
        let Some(id) = node_id(node) else {
            continue;
        };
        let Some(file) = node_file(node, &id) else {
            continue;
        };
        file_of.insert(id.clone(), file.clone());
        if file == path {
            on_path.push(id);
        }
    }
    if !is_test_source_path(&path) {
        let evidence_paths = on_path
            .iter()
            .take(MAX_EVIDENCE_PATHS)
            .map(|id| vec![id.clone()])
            .collect();
        return BindingReach {
            truncated: false,
            nodes: on_path,
            evidence_paths,
        };
    }
    let outgoing = outgoing(graph);
    let mut seen = BTreeSet::<String>::new();
    let mut out = BTreeSet::<String>::new();
    let mut evidence_paths = Vec::new();
    let mut truncated = false;
    let mut queue = VecDeque::new();
    for id in on_path {
        seen.insert(id.clone());
        queue.push_back((id.clone(), vec![id], 0_usize));
    }
    while let Some((current, trail, depth)) = queue.pop_front() {
        if depth >= MAX_REACH_DEPTH {
            truncated = true;
            continue;
        }
        if out.len() >= MAX_REACH_NODES {
            truncated = true;
            break;
        }
        let Some(neighbours) = outgoing.get(&current) else {
            continue;
        };
        for next in neighbours {
            if !seen.insert(next.clone()) {
                continue;
            }
            if let Some(file) = file_of.get(next)
                && is_test_source_path(file)
            {
                continue;
            }
            let mut next_trail = trail.clone();
            next_trail.push(next.clone());
            if file_of
                .get(next)
                .is_some_and(|file| is_production_source(file))
            {
                if out.len() >= MAX_REACH_NODES {
                    truncated = true;
                    break;
                }
                out.insert(next.clone());
                if evidence_paths.len() < MAX_EVIDENCE_PATHS {
                    evidence_paths.push(next_trail.clone());
                } else {
                    truncated = true;
                }
            }
            if depth + 1 < MAX_REACH_DEPTH {
                queue.push_back((next.clone(), next_trail, depth + 1));
            } else if outgoing.get(next).is_some_and(|set| !set.is_empty()) {
                truncated = true;
            }
        }
    }
    BindingReach {
        nodes: out.into_iter().collect(),
        evidence_paths,
        truncated,
    }
}

fn outgoing(graph: &Value) -> BTreeMap<String, BTreeSet<String>> {
    let mut out = BTreeMap::<String, BTreeSet<String>>::new();
    for edge in values(graph, "edges") {
        let Some(source) = edge_end(edge, "source").or_else(|| edge_end(edge, "from")) else {
            continue;
        };
        let Some(target) = edge_end(edge, "target").or_else(|| edge_end(edge, "to")) else {
            continue;
        };
        out.entry(source).or_default().insert(target);
    }
    out
}

fn node_file(node: &Value, id: &str) -> Option<String> {
    node.pointer("/span/file")
        .and_then(Value::as_str)
        .map(normalize_path)
        .or_else(|| node_source_path(id).map(normalize_path))
        .filter(|path| !path.is_empty())
}

fn node_source_path(node_id: &str) -> Option<&str> {
    let raw = node_id
        .strip_prefix("file:")
        .or_else(|| node_id.strip_prefix("symbol:"))
        .unwrap_or(node_id);
    let path = raw.split('#').next().unwrap_or(raw);
    (!path.is_empty()).then_some(path)
}

fn is_production_source(path: &str) -> bool {
    if is_test_source_path(path) {
        return false;
    }
    let lower = path.to_ascii_lowercase();
    [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".go", ".rs"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

fn is_test_source_path(path: &str) -> bool {
    let path = normalize_path(path).to_ascii_lowercase();
    let file = path.rsplit('/').next().unwrap_or(&path);
    path.contains("/tests/")
        || path.starts_with("tests/")
        || path.contains("/__tests__/")
        || file.ends_with("_test.go")
        || file.starts_with("test_")
        || file.contains(".test.")
        || file.contains(".spec.")
        || file.contains(".stories.")
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn values<'a>(value: &'a Value, key: &str) -> Vec<&'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn node_id(node: &Value) -> Option<String> {
    node.get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

fn edge_end(edge: &Value, key: &str) -> Option<String> {
    edge.get(key)
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}
