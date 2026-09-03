//! Behavior surfaces sit on top of the Application Surface Graph.
//!
//! Combinations exist only when evidenced. Two facts are never crossed into a
//! third. A fact whose base surface is not in the application graph is dropped.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::surface_graph::ApplicationSurfaceGraph;

/// Hard ceiling on evidenced behavior combinations in one projection.
pub const MAX_BEHAVIOR_SURFACES: usize = 512;

/// Why a behavior combination is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorSurfaceOrigin {
    /// Named in `OpenSpec` / `quality.yaml`.
    Declared,
    /// Seen in a live browser observation.
    Observed,
    /// Admitted by a continuous journal.
    Recorded,
    /// Backed by a Storybook story.
    Story,
    /// Reachable on the Weavatrix graph with evidence.
    Graph,
}

/// One evidenced (surface, role, state, action, flag) tuple.
///
/// Missing dimensions stay `None`. This is not a cell in a full matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorSurfaceFact {
    /// Application surface id (`route:/checkout`).
    pub surface: String,
    /// Actor / auth role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Semantic state (`empty_cart`, modal name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Observed or declared action kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Feature-flag identity (`dark=on`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flag: Option<String>,
    /// How this tuple entered the projection.
    pub origin: BehaviorSurfaceOrigin,
}

impl BehaviorSurfaceFact {
    fn has_dimension(&self) -> bool {
        self.role.is_some() || self.state.is_some() || self.action.is_some() || self.flag.is_some()
    }

    fn identity(&self) -> Option<String> {
        if !self.has_dimension() || contains_separator(&self.surface) {
            return None;
        }
        for part in [&self.role, &self.state, &self.action, &self.flag]
            .into_iter()
            .flatten()
        {
            if part.trim().is_empty() || contains_separator(part) {
                return None;
            }
        }
        let mut id = self.surface.clone();
        push_dim(&mut id, "role", self.role.as_deref());
        push_dim(&mut id, "state", self.state.as_deref());
        push_dim(&mut id, "action", self.action.as_deref());
        push_dim(&mut id, "flag", self.flag.as_deref());
        Some(id)
    }
}

/// One evidenced behavior combination hanging off an application surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorSurface {
    /// Stable identity. Only dimensions that were evidenced appear.
    pub id: String,
    /// Application surface this combination belongs to.
    pub surface: String,
    /// Actor / auth role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Semantic state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Action kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Feature-flag identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flag: Option<String>,
    /// Provenance, strongest first, unique.
    pub origins: Vec<BehaviorSurfaceOrigin>,
}

/// Evidenced behavior combinations. Never a Cartesian product.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorSurfaceGraph {
    /// Combinations, sorted by id.
    pub behaviors: Vec<BehaviorSurface>,
    /// True when [`MAX_BEHAVIOR_SURFACES`] stopped the list.
    pub truncated: bool,
}

/// Project evidenced facts onto existing application surfaces.
///
/// Facts are not crossed. A fact with no extra dimension is ignored — that
/// identity already lives on the application graph.
#[must_use]
pub fn behavior_surface_graph(
    surfaces: &ApplicationSurfaceGraph,
    facts: &[BehaviorSurfaceFact],
) -> BehaviorSurfaceGraph {
    let known = surfaces
        .surfaces
        .iter()
        .map(|surface| surface.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut by_id = BTreeMap::<String, BehaviorSurface>::new();
    for fact in facts {
        if !known.contains(fact.surface.as_str()) {
            continue;
        }
        let Some(id) = fact.identity() else {
            continue;
        };
        match by_id.get_mut(&id) {
            Some(existing) => {
                if !existing.origins.contains(&fact.origin) {
                    existing.origins.push(fact.origin);
                    existing.origins.sort();
                    existing.origins.dedup();
                }
            }
            None => {
                by_id.insert(
                    id.clone(),
                    BehaviorSurface {
                        id,
                        surface: fact.surface.clone(),
                        role: fact.role.clone(),
                        state: fact.state.clone(),
                        action: fact.action.clone(),
                        flag: fact.flag.clone(),
                        origins: vec![fact.origin],
                    },
                );
            }
        }
    }
    let truncated = by_id.len() > MAX_BEHAVIOR_SURFACES;
    let mut behaviors = by_id.into_values().collect::<Vec<_>>();
    behaviors.sort_by(|left, right| left.id.cmp(&right.id));
    if truncated {
        behaviors.truncate(MAX_BEHAVIOR_SURFACES);
    }
    BehaviorSurfaceGraph {
        behaviors,
        truncated,
    }
}

fn push_dim(id: &mut String, name: &str, value: Option<&str>) {
    if let Some(value) = value {
        id.push('|');
        id.push_str(name);
        id.push(':');
        id.push_str(value.trim());
    }
}

fn contains_separator(value: &str) -> bool {
    value.contains('|')
}
