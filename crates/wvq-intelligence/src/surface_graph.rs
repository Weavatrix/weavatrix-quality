//! Application Surface Graph: a projection of Weavatrix, not a second parser.
//!
//! Routes, endpoints, operations, events, and public APIs are named surfaces.
//! Implementation nodes hang off them. Test/spec/Storybook files never become
//! a production surface.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Hard ceiling on named surfaces in one projection.
pub const MAX_APPLICATION_SURFACES: usize = 512;

/// Kind of externally visible surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationSurfaceKind {
    /// UI route.
    Route,
    /// UI/component identity.
    Component,
    /// HTTP endpoint.
    Endpoint,
    /// GraphQL/gRPC operation.
    Operation,
    /// Event producer or consumer.
    Event,
    /// Public language-level API.
    PublicApi,
}

/// How a surface entered the graph. Stronger kinds win.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceEvidenceKind {
    /// Weavatrix `list_endpoints` (or equivalent) named it.
    WeavatrixEndpoint,
    /// A Weavatrix node kind/id named it.
    WeavatrixNode,
    /// Heuristic label (`GET /…`, `/checkout`). Weakest.
    HeuristicLabel,
}

/// One named production surface and the implementation nodes it may claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationSurface {
    /// Stable identity, e.g. `endpoint:GET /pay` or `route:/checkout`.
    pub id: String,
    /// Surface kind.
    pub kind: ApplicationSurfaceKind,
    /// Implementation Weavatrix nodes. Test nodes are excluded.
    pub implementation_nodes: Vec<String>,
    /// Provenance. Stronger kinds replace weaker ones for the same id.
    pub evidence: SurfaceEvidenceKind,
}

/// Projection of one Weavatrix graph onto application surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationSurfaceGraph {
    /// Named surfaces, sorted by id.
    pub surfaces: Vec<ApplicationSurface>,
    /// True when [`MAX_APPLICATION_SURFACES`] stopped the list.
    pub truncated: bool,
}

/// Project Weavatrix `nodes` / `edges` / optional `endpoints` onto surfaces.
#[must_use]
pub fn application_surface_graph(graph: &Value) -> ApplicationSurfaceGraph {
    let mut by_id = BTreeMap::<String, ApplicationSurface>::new();
    let mut truncated = false;

    for item in values(graph, "endpoints") {
        let Some(raw) = endpoint_name(item) else {
            continue;
        };
        if is_test_source_path(&raw) {
            continue;
        }
        let id = format!("endpoint:{raw}");
        let mut impl_nodes = Vec::new();
        if let Some(handler) = item.get("handler").and_then(Value::as_str)
            && !is_test_source_path(handler)
        {
            impl_nodes.push(handler.to_owned());
        }
        for node in values(item, "nodes") {
            if let Some(node_id) = node_id(node)
                && !node_is_test(&node_id)
            {
                impl_nodes.push(node_id);
            }
        }
        upsert(
            &mut by_id,
            ApplicationSurface {
                id,
                kind: ApplicationSurfaceKind::Endpoint,
                implementation_nodes: impl_nodes,
                evidence: SurfaceEvidenceKind::WeavatrixEndpoint,
            },
        );
    }

    let mut adjacency = BTreeMap::<String, BTreeSet<String>>::new();
    for edge in values(graph, "edges") {
        let Some(source) = edge_end(edge, "source").or_else(|| edge_end(edge, "from")) else {
            continue;
        };
        let Some(target) = edge_end(edge, "target").or_else(|| edge_end(edge, "to")) else {
            continue;
        };
        adjacency.entry(source.clone()).or_default().insert(target.clone());
        adjacency.entry(target).or_default().insert(source);
    }

    for node in values(graph, "nodes") {
        let Some(id) = node_id(node) else {
            continue;
        };
        if node_is_test(&id) {
            continue;
        }
        let Some((kind, surface_id, evidence)) = classify_surface(&id, node) else {
            continue;
        };
        let mut impl_nodes = neighbors(&id, &adjacency)
            .into_iter()
            .filter(|candidate| !node_is_test(candidate) && is_implementation(candidate))
            .collect::<Vec<_>>();
        if is_implementation(&id) {
            impl_nodes.push(id);
        }
        upsert(
            &mut by_id,
            ApplicationSurface {
                id: surface_id,
                kind,
                implementation_nodes: impl_nodes,
                evidence,
            },
        );
    }

    let mut surfaces = by_id.into_values().collect::<Vec<_>>();
    for surface in &mut surfaces {
        surface.implementation_nodes.sort();
        surface.implementation_nodes.dedup();
    }
    surfaces.sort_by(|left, right| left.id.cmp(&right.id));
    if surfaces.len() > MAX_APPLICATION_SURFACES {
        surfaces.truncate(MAX_APPLICATION_SURFACES);
        truncated = true;
    }
    ApplicationSurfaceGraph {
        surfaces,
        truncated,
    }
}

fn upsert(by_id: &mut BTreeMap<String, ApplicationSurface>, incoming: ApplicationSurface) {
    match by_id.get_mut(&incoming.id) {
        Some(existing) => {
            if incoming.evidence < existing.evidence {
                existing.evidence = incoming.evidence;
            }
            existing
                .implementation_nodes
                .extend(incoming.implementation_nodes);
        }
        None => {
            by_id.insert(incoming.id.clone(), incoming);
        }
    }
}

fn classify_surface(
    id: &str,
    node: &Value,
) -> Option<(ApplicationSurfaceKind, String, SurfaceEvidenceKind)> {
    let kind = node.get("kind").and_then(Value::as_str).unwrap_or("");
    let label = node.get("label").and_then(Value::as_str).unwrap_or(id);
    match kind {
        "endpoint" => Some((
            ApplicationSurfaceKind::Endpoint,
            format!("endpoint:{label}"),
            SurfaceEvidenceKind::WeavatrixNode,
        )),
        "route" => Some((
            ApplicationSurfaceKind::Route,
            format!("route:{label}"),
            SurfaceEvidenceKind::WeavatrixNode,
        )),
        "component" => Some((
            ApplicationSurfaceKind::Component,
            format!("component:{label}"),
            SurfaceEvidenceKind::WeavatrixNode,
        )),
        "operation" => Some((
            ApplicationSurfaceKind::Operation,
            format!("operation:{label}"),
            SurfaceEvidenceKind::WeavatrixNode,
        )),
        "event" => Some((
            ApplicationSurfaceKind::Event,
            format!("event:{label}"),
            SurfaceEvidenceKind::WeavatrixNode,
        )),
        _ if looks_like_endpoint(label) || looks_like_endpoint(id) => {
            let name = if looks_like_endpoint(label) {
                label
            } else {
                id
            };
            Some((
                ApplicationSurfaceKind::Endpoint,
                format!("endpoint:{name}"),
                SurfaceEvidenceKind::HeuristicLabel,
            ))
        }
        _ if looks_like_route(label) => Some((
            ApplicationSurfaceKind::Route,
            format!("route:{label}"),
            SurfaceEvidenceKind::HeuristicLabel,
        )),
        _ => None,
    }
}

fn looks_like_endpoint(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("GET ")
        || trimmed.starts_with("POST ")
        || trimmed.starts_with("PUT ")
        || trimmed.starts_with("PATCH ")
        || trimmed.starts_with("DELETE ")
}

fn looks_like_route(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with('/') && !trimmed.starts_with("//") && !trimmed.contains('.')
}

fn is_implementation(id: &str) -> bool {
    let path = node_source_path(id).unwrap_or(id);
    (id.starts_with("symbol:") || id.starts_with("file:") || id.contains('#'))
        && !is_test_source_path(path)
}

fn node_is_test(id: &str) -> bool {
    is_test_source_path(node_source_path(id).unwrap_or(id))
}

fn node_source_path(node_id: &str) -> Option<&str> {
    let raw = node_id
        .strip_prefix("file:")
        .or_else(|| node_id.strip_prefix("symbol:"))
        .unwrap_or(node_id);
    let path = raw.split('#').next().unwrap_or(raw);
    (!path.is_empty()).then_some(path)
}

fn is_test_source_path(path: &str) -> bool {
    let path = path.replace('\\', "/").to_ascii_lowercase();
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

fn neighbors(id: &str, adjacency: &BTreeMap<String, BTreeSet<String>>) -> Vec<String> {
    adjacency
        .get(id)
        .map(|set| set.iter().cloned().collect())
        .unwrap_or_default()
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

fn endpoint_name(item: &Value) -> Option<String> {
    item.as_str()
        .or_else(|| item.get("label").and_then(Value::as_str))
        .or_else(|| item.get("id").and_then(Value::as_str))
        .map(str::to_owned)
}
