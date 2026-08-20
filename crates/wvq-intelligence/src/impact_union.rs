//! Spec §68: never compute impact only on head.
//!
//! If a change deletes a function, an edge, a route or a handler, the head graph
//! no longer contains the old path. An algorithm that only looks at the final
//! graph cannot see that a validation step disappeared, so the impacted surface
//! is always a union of both revisions plus what was removed outright.

use std::collections::BTreeSet;

use serde::Serialize;

/// What `graph_diff` reported as gone on head.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphDelta {
    /// Nodes present on base, absent on head.
    pub removed_nodes: Vec<String>,
    /// Edges present on base, absent on head, as `from->to`.
    pub removed_edges: Vec<String>,
}

/// Externally visible surfaces added or removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SurfaceDelta {
    /// Surfaces the head revision added.
    pub added: Vec<String>,
    /// Surfaces the head revision removed.
    pub removed: Vec<String>,
}

/// The dual-revision impacted surface, kept in separate buckets.
///
/// The buckets are stored apart on purpose: "this only existed before the
/// change" is a different and more dangerous fact than "this is new".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ImpactedSurface {
    /// Impacted on base only.
    pub base_only: Vec<String>,
    /// Impacted on head only.
    pub head_only: Vec<String>,
    /// Impacted on both.
    pub shared: Vec<String>,
    /// Nodes the change removed.
    pub removed_nodes: Vec<String>,
    /// Edges the change removed.
    pub removed_edges: Vec<String>,
    /// Public surfaces the change removed.
    pub removed_surfaces: Vec<String>,
}

impl ImpactedSurface {
    /// Every impacted node, from either revision, plus what was removed.
    ///
    /// This is what selection and protection analysis must range over.
    #[must_use]
    pub fn all_nodes(&self) -> Vec<String> {
        let mut out: BTreeSet<&String> = BTreeSet::new();
        for list in [
            &self.base_only,
            &self.head_only,
            &self.shared,
            &self.removed_nodes,
        ] {
            out.extend(list.iter());
        }
        out.into_iter().cloned().collect()
    }

    /// Whether the change removed anything at all.
    #[must_use]
    pub fn has_removals(&self) -> bool {
        !self.removed_nodes.is_empty()
            || !self.removed_edges.is_empty()
            || !self.removed_surfaces.is_empty()
    }

    /// How many nodes a head-only algorithm would have missed.
    #[must_use]
    pub fn missed_by_head_only(&self) -> usize {
        let head: BTreeSet<&String> = self.head_only.iter().chain(self.shared.iter()).collect();
        self.base_only
            .iter()
            .chain(self.removed_nodes.iter())
            .collect::<BTreeSet<&String>>()
            .into_iter()
            .filter(|item| !head.contains(*item))
            .count()
    }
}

/// Build the impacted surface from both revisions.
///
/// `ImpactedSurface = Impact(base) ∪ Impact(head) ∪ removed nodes ∪ removed
/// edges ∪ removed public surfaces`.
#[must_use]
pub fn impacted_surface(
    base_impact: &[String],
    head_impact: &[String],
    graph: &GraphDelta,
    surfaces: &SurfaceDelta,
) -> ImpactedSurface {
    let base: BTreeSet<&String> = base_impact.iter().collect();
    let head: BTreeSet<&String> = head_impact.iter().collect();

    ImpactedSurface {
        base_only: base.difference(&head).map(|item| (*item).clone()).collect(),
        head_only: head.difference(&base).map(|item| (*item).clone()).collect(),
        shared: base
            .intersection(&head)
            .map(|item| (*item).clone())
            .collect(),
        removed_nodes: unique(&graph.removed_nodes),
        removed_edges: unique(&graph.removed_edges),
        removed_surfaces: unique(&surfaces.removed),
    }
}

fn unique(values: &[String]) -> Vec<String> {
    let set: BTreeSet<&String> = values.iter().collect();
    set.into_iter().cloned().collect()
}
