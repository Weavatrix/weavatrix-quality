//! Reach from a declared binding path to production Weavatrix nodes.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde_json::Value;

const MAX_REACH_DEPTH: usize = 4;
const MAX_REACH_NODES: usize = 64;

/// Production implementation nodes a binding may claim.
///
/// A production file binding returns the nodes in that file. A test/spec
/// binding follows graph edges (bounded) onto production source nodes and
/// never returns the test node itself.
#[must_use]
pub fn production_nodes_for_binding(graph: &Value, binding_path: &str) -> Vec<String> {
    let path = normalize_path(binding_path);
    if path.is_empty() {
        return Vec::new();
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
        return on_path;
    }
    let adjacency = adjacency(graph);
    let mut seen = BTreeSet::<String>::new();
    let mut out = BTreeSet::<String>::new();
    let mut queue = VecDeque::new();
    for id in on_path {
        seen.insert(id.clone());
        queue.push_back((id, 0_usize));
    }
    while let Some((current, depth)) = queue.pop_front() {
        if depth >= MAX_REACH_DEPTH || out.len() >= MAX_REACH_NODES {
            break;
        }
        let Some(neighbours) = adjacency.get(&current) else {
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
            if file_of
                .get(next)
                .is_some_and(|file| is_production_source(file))
            {
                out.insert(next.clone());
            }
            if depth + 1 < MAX_REACH_DEPTH {
                queue.push_back((next.clone(), depth + 1));
            }
        }
    }
    out.into_iter().collect()
}

fn adjacency(graph: &Value) -> BTreeMap<String, BTreeSet<String>> {
    let mut out = BTreeMap::<String, BTreeSet<String>>::new();
    for edge in values(graph, "edges") {
        let Some(source) = edge_end(edge, "source").or_else(|| edge_end(edge, "from")) else {
            continue;
        };
        let Some(target) = edge_end(edge, "target").or_else(|| edge_end(edge, "to")) else {
            continue;
        };
        out.entry(source.clone()).or_default().insert(target.clone());
        out.entry(target).or_default().insert(source);
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
