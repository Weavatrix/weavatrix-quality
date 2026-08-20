//! Spec §69–§70 impacted flows, fingerprints and lineage.
//!
//! A flow is a projection of Weavatrix graph evidence, not a second graph. It
//! needs continuity across revisions even when the implementation is refactored,
//! so matching prefers stable surfaces and requirements over file paths.

use std::collections::BTreeSet;

use serde::Serialize;

/// Where a flow starts.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowEntry {
    /// UI route or component interaction.
    Route(String),
    /// HTTP endpoint.
    Endpoint(String),
    /// GraphQL or gRPC operation.
    Operation(String),
    /// Event consumer or producer.
    Event(String),
    /// Public language-level API.
    PublicApi(String),
}

impl FlowEntry {
    /// The stable surface identity this entry represents.
    #[must_use]
    pub fn surface(&self) -> &str {
        match self {
            Self::Route(value)
            | Self::Endpoint(value)
            | Self::Operation(value)
            | Self::Event(value)
            | Self::PublicApi(value) => value,
        }
    }
}

/// One impacted flow at one revision. Spec §69.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImpactedFlow {
    /// Flow identity within its revision.
    pub id: String,
    /// Revision this projection belongs to.
    pub revision: String,
    /// Entry surface.
    pub entry: FlowEntry,
    /// Graph nodes the flow passes through.
    pub graph_nodes: Vec<String>,
    /// Graph edges, as `from->to`.
    pub graph_edges: Vec<String>,
    /// Public surfaces the flow exposes.
    pub public_surfaces: Vec<String>,
    /// Requirements the flow is known to serve.
    pub requirements: Vec<String>,
}

/// Spec §70 fingerprint. Deliberately excludes file paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlowFingerprint {
    /// Stable entry surface.
    pub entry_surface: String,
    /// Capability, when known.
    pub capability: Option<String>,
    /// Observable contract atoms, sorted.
    pub observable_contract: Vec<String>,
    /// Digest of the structural shape.
    pub structural_digest: String,
}

/// Fingerprint one flow.
#[must_use]
pub fn fingerprint(flow: &ImpactedFlow, capability: Option<String>) -> FlowFingerprint {
    let mut contract: Vec<String> = flow.public_surfaces.clone();
    contract.extend(flow.requirements.clone());
    contract.sort();
    contract.dedup();
    let mut nodes = flow.graph_nodes.clone();
    nodes.sort();
    FlowFingerprint {
        entry_surface: flow.entry.surface().to_owned(),
        capability,
        observable_contract: contract,
        structural_digest: nodes.join("|"),
    }
}

/// What happened to a flow between two revisions. Spec §70.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowState {
    /// Same nodes, same edges.
    Unchanged,
    /// Same entry, different nodes.
    Modified,
    /// Same nodes, different edges: the path through them changed.
    Rewired,
    /// One base flow became several head flows.
    Split,
    /// Several base flows became one head flow.
    Merged,
    /// Head-only.
    Added,
    /// Base-only. Invisible to any head-only algorithm.
    Removed,
    /// Continuity could not be established. WVQ does not guess.
    Unmatched,
}

impl FlowState {
    /// Stable token for transport.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unchanged => "unchanged",
            Self::Modified => "modified",
            Self::Rewired => "rewired",
            Self::Split => "split",
            Self::Merged => "merged",
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Unmatched => "unmatched",
        }
    }
}

/// One matched pair, or an unmatched side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlowMatch {
    /// Lineage state.
    pub state: FlowState,
    /// Base flow id, when there is one.
    pub base: Option<String>,
    /// Head flow id, when there is one.
    pub head: Option<String>,
    /// Why the two sides were matched.
    pub matched_on: &'static str,
    /// Stable surface the pair is about.
    pub surface: String,
}

/// Match base flows against head flows.
///
/// Preference order follows spec §70: the same entry surface first, then a
/// shared requirement, then graph-neighbourhood similarity. File paths are never
/// used, so a refactor that moves code keeps its lineage.
#[must_use]
pub fn match_flows(base: &[ImpactedFlow], head: &[ImpactedFlow]) -> Vec<FlowMatch> {
    let mut out = Vec::new();
    let mut used_head: BTreeSet<usize> = BTreeSet::new();

    for base_flow in base {
        let candidates: Vec<usize> = head
            .iter()
            .enumerate()
            .filter(|(index, _)| !used_head.contains(index))
            .filter(|(_, head_flow)| head_flow.entry.surface() == base_flow.entry.surface())
            .map(|(index, _)| index)
            .collect();

        if candidates.len() > 1 {
            for index in &candidates {
                used_head.insert(*index);
                out.push(FlowMatch {
                    state: FlowState::Split,
                    base: Some(base_flow.id.clone()),
                    head: Some(head[*index].id.clone()),
                    matched_on: "entry_surface",
                    surface: base_flow.entry.surface().to_owned(),
                });
            }
            continue;
        }
        if let Some(index) = candidates.first().copied() {
            used_head.insert(index);
            out.push(pair(base_flow, &head[index], "entry_surface"));
            continue;
        }

        // The surface is gone. Try a shared requirement, then node overlap.
        let by_requirement = head.iter().enumerate().find(|(index, head_flow)| {
            !used_head.contains(index)
                && head_flow
                    .requirements
                    .iter()
                    .any(|item| base_flow.requirements.contains(item))
        });
        if let Some((index, head_flow)) = by_requirement {
            used_head.insert(index);
            out.push(pair(base_flow, head_flow, "requirement"));
            continue;
        }
        let by_nodes = head.iter().enumerate().find(|(index, head_flow)| {
            !used_head.contains(index) && node_overlap(base_flow, head_flow) >= 2
        });
        if let Some((index, head_flow)) = by_nodes {
            used_head.insert(index);
            out.push(pair(base_flow, head_flow, "graph_neighbourhood"));
            continue;
        }

        out.push(FlowMatch {
            state: FlowState::Removed,
            base: Some(base_flow.id.clone()),
            head: None,
            matched_on: "none",
            surface: base_flow.entry.surface().to_owned(),
        });
    }

    for (index, head_flow) in head.iter().enumerate() {
        if used_head.contains(&index) {
            continue;
        }
        out.push(FlowMatch {
            state: FlowState::Added,
            base: None,
            head: Some(head_flow.id.clone()),
            matched_on: "none",
            surface: head_flow.entry.surface().to_owned(),
        });
    }

    // Several base flows matched to one head flow means a merge.
    let mut merged = Vec::new();
    for item in &out {
        if let Some(head_id) = &item.head
            && out
                .iter()
                .filter(|other| other.head.as_ref() == Some(head_id) && other.base.is_some())
                .count()
                > 1
        {
            merged.push(head_id.clone());
        }
    }
    for item in &mut out {
        if let Some(head_id) = &item.head
            && merged.contains(head_id)
            && item.base.is_some()
        {
            item.state = FlowState::Merged;
        }
    }

    out.sort_by(|left, right| {
        left.surface
            .cmp(&right.surface)
            .then_with(|| left.base.cmp(&right.base))
            .then_with(|| left.head.cmp(&right.head))
    });
    out
}

fn pair(base: &ImpactedFlow, head: &ImpactedFlow, matched_on: &'static str) -> FlowMatch {
    let same_nodes = sorted(&base.graph_nodes) == sorted(&head.graph_nodes);
    let same_edges = sorted(&base.graph_edges) == sorted(&head.graph_edges);
    let state = match (same_nodes, same_edges) {
        (true, true) => FlowState::Unchanged,
        (true, false) => FlowState::Rewired,
        _ => FlowState::Modified,
    };
    FlowMatch {
        state,
        base: Some(base.id.clone()),
        head: Some(head.id.clone()),
        matched_on,
        surface: base.entry.surface().to_owned(),
    }
}

fn node_overlap(left: &ImpactedFlow, right: &ImpactedFlow) -> usize {
    let right_nodes: BTreeSet<&String> = right.graph_nodes.iter().collect();
    left.graph_nodes
        .iter()
        .filter(|item| right_nodes.contains(item))
        .count()
}

fn sorted(values: &[String]) -> Vec<&String> {
    let mut out: Vec<&String> = values.iter().collect();
    out.sort();
    out
}
