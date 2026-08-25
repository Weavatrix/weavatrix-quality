//! Coverage Autopilot: name which application surfaces lack measured evidence.
//!
//! Missing coverage is unmeasured, never a fake uncovered. Global percentages
//! do not override a local surface gap.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::surface_graph::{ApplicationSurfaceGraph, ApplicationSurfaceKind};
use crate::{CoverageMeasurement, NodeCoverage};

/// How one production surface relates to measured coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceCoverageState {
    /// At least one implementation node was hit.
    MeasuredCovered,
    /// Implementation nodes are instrumented and every hit count is zero.
    MeasuredUncovered,
    /// No measured report covers this surface. Not evidence of absence.
    Unmeasured,
}

/// Coverage reading for one named surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceCoverage {
    /// Surface id from the Application Surface Graph.
    pub surface: String,
    /// Surface kind.
    pub kind: ApplicationSurfaceKind,
    /// Measurement kind.
    pub state: SurfaceCoverageState,
    /// Implementation nodes that were hit.
    pub covered_nodes: u64,
    /// Instrumented implementation nodes with zero hits.
    pub uncovered_nodes: u64,
    /// Implementation nodes with no measured report.
    pub unmeasured_nodes: u64,
}

/// Autopilot reading: which surfaces still need evidence.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageAutopilot {
    /// Every named production surface, sorted by id.
    pub surfaces: Vec<SurfaceCoverage>,
    /// Surfaces with no measured report.
    pub unmeasured: Vec<String>,
    /// Surfaces instrumented and entirely unhit.
    pub uncovered: Vec<String>,
    /// Surfaces with at least one hit.
    pub covered: Vec<String>,
}

/// Classify each application surface against node-level coverage.
#[must_use]
pub fn coverage_autopilot(
    graph: &ApplicationSurfaceGraph,
    coverage: &[NodeCoverage],
) -> CoverageAutopilot {
    let by_node = coverage
        .iter()
        .map(|item| (item.node_id.as_str(), item.measurement))
        .collect::<BTreeMap<_, _>>();
    let mut out = CoverageAutopilot::default();
    for surface in &graph.surfaces {
        let mut covered = 0_u64;
        let mut uncovered = 0_u64;
        let mut unmeasured = 0_u64;
        if surface.implementation_nodes.is_empty() {
            unmeasured = 1;
        }
        for node in &surface.implementation_nodes {
            match by_node.get(node.as_str()).copied() {
                Some(CoverageMeasurement::Covered) => covered = covered.saturating_add(1),
                Some(CoverageMeasurement::Uncovered) => uncovered = uncovered.saturating_add(1),
                Some(CoverageMeasurement::Unmeasured) | None => {
                    unmeasured = unmeasured.saturating_add(1);
                }
            }
        }
        let state = if covered > 0 {
            SurfaceCoverageState::MeasuredCovered
        } else if uncovered > 0 && unmeasured == 0 {
            SurfaceCoverageState::MeasuredUncovered
        } else {
            SurfaceCoverageState::Unmeasured
        };
        let reading = SurfaceCoverage {
            surface: surface.id.clone(),
            kind: surface.kind,
            state,
            covered_nodes: covered,
            uncovered_nodes: uncovered,
            unmeasured_nodes: unmeasured,
        };
        match state {
            SurfaceCoverageState::MeasuredCovered => out.covered.push(surface.id.clone()),
            SurfaceCoverageState::MeasuredUncovered => out.uncovered.push(surface.id.clone()),
            SurfaceCoverageState::Unmeasured => out.unmeasured.push(surface.id.clone()),
        }
        out.surfaces.push(reading);
    }
    out.surfaces.sort_by(|left, right| left.surface.cmp(&right.surface));
    out.covered.sort();
    out.uncovered.sort();
    out.unmeasured.sort();
    out
}
