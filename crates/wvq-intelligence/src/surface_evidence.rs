//! Surface Evidence Matrix: which evidence kinds exist for each surface.
//!
//! This is Coverage Autopilot input, not a verdict axis. A missing producer is
//! unmeasured, never a fake absent. Present and absent are only used when that
//! column was actually measured.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::surface_graph::{ApplicationSurfaceGraph, ApplicationSurfaceKind};

/// One measured evidence column. Surfaces in neither set stay unmeasured.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredColumn {
    /// Surfaces that have this evidence kind.
    pub present: BTreeSet<String>,
    /// Surfaces measured without this evidence kind.
    pub absent: BTreeSet<String>,
}

impl MeasuredColumn {
    /// Every named surface is present or absent. Nothing stays unmeasured.
    #[must_use]
    pub fn closed_world(graph: &ApplicationSurfaceGraph, present: BTreeSet<String>) -> Self {
        let absent = graph
            .surfaces
            .iter()
            .map(|surface| surface.id.clone())
            .filter(|id| !present.contains(id))
            .collect();
        Self { present, absent }
    }
}

/// Measured present/absent sets for each matrix column. `None` is unmeasured.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SurfaceEvidenceColumns {
    /// `OpenSpec` / obligation intent bound to the surface.
    pub intent: Option<MeasuredColumn>,
    /// Runtime behavior observations.
    pub runtime: Option<MeasuredColumn>,
    /// A normalized test that reached the surface.
    pub test: Option<MeasuredColumn>,
    /// An assembled Proof for an obligation on the surface.
    pub proof: Option<MeasuredColumn>,
    /// Protection / coverage measurement.
    pub protection: Option<MeasuredColumn>,
    /// UI-integrity measurement.
    pub ui: Option<MeasuredColumn>,
    /// Accessibility measurement.
    pub a11y: Option<MeasuredColumn>,
    /// Source-mutation measurement.
    pub mutation: Option<MeasuredColumn>,
}

/// Whether one evidence kind was observed for a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceCell {
    /// This producer ran and named the surface.
    Present,
    /// This producer ran and did not name the surface.
    Absent,
    /// This producer was not measured. Not evidence of absence.
    Unmeasured,
}

/// One surface's evidence row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceEvidenceRow {
    /// Surface id from the Application Surface Graph.
    pub surface: String,
    /// Surface kind.
    pub kind: ApplicationSurfaceKind,
    /// `OpenSpec` / obligation intent.
    pub intent: EvidenceCell,
    /// Runtime behavior.
    pub runtime: EvidenceCell,
    /// Normalized test.
    pub test: EvidenceCell,
    /// Assembled Proof.
    pub proof: EvidenceCell,
    /// Protection / coverage.
    pub protection: EvidenceCell,
    /// UI integrity.
    pub ui: EvidenceCell,
    /// Accessibility.
    pub a11y: EvidenceCell,
    /// Source mutation.
    pub mutation: EvidenceCell,
}

/// Matrix over every named production surface.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceEvidenceMatrix {
    /// One row per surface, sorted by id.
    pub surfaces: Vec<SurfaceEvidenceRow>,
    /// True when the surface graph itself was truncated.
    pub truncated: bool,
}

/// Classify each application surface against measured evidence columns.
#[must_use]
pub fn surface_evidence_matrix(
    graph: &ApplicationSurfaceGraph,
    columns: &SurfaceEvidenceColumns,
) -> SurfaceEvidenceMatrix {
    let mut surfaces = graph
        .surfaces
        .iter()
        .map(|surface| SurfaceEvidenceRow {
            surface: surface.id.clone(),
            kind: surface.kind,
            intent: cell(columns.intent.as_ref(), &surface.id),
            runtime: cell(columns.runtime.as_ref(), &surface.id),
            test: cell(columns.test.as_ref(), &surface.id),
            proof: cell(columns.proof.as_ref(), &surface.id),
            protection: cell(columns.protection.as_ref(), &surface.id),
            ui: cell(columns.ui.as_ref(), &surface.id),
            a11y: cell(columns.a11y.as_ref(), &surface.id),
            mutation: cell(columns.mutation.as_ref(), &surface.id),
        })
        .collect::<Vec<_>>();
    surfaces.sort_by(|left, right| left.surface.cmp(&right.surface));
    SurfaceEvidenceMatrix {
        surfaces,
        truncated: graph.truncated,
    }
}

/// Surfaces whose implementation nodes intersect `nodes`.
#[must_use]
pub fn surfaces_touching_nodes<'a, I, S>(graph: &ApplicationSurfaceGraph, nodes: I) -> BTreeSet<String>
where
    I: IntoIterator<Item = &'a S>,
    S: AsRef<str> + 'a,
{
    let wanted = nodes
        .into_iter()
        .map(|node| node.as_ref().to_owned())
        .collect::<BTreeSet<_>>();
    if wanted.is_empty() {
        return BTreeSet::new();
    }
    graph
        .surfaces
        .iter()
        .filter(|surface| {
            surface
                .implementation_nodes
                .iter()
                .any(|node| wanted.contains(node))
        })
        .map(|surface| surface.id.clone())
        .collect()
}

fn cell(column: Option<&MeasuredColumn>, surface: &str) -> EvidenceCell {
    let Some(column) = column else {
        return EvidenceCell::Unmeasured;
    };
    match (
        column.present.contains(surface),
        column.absent.contains(surface),
    ) {
        (true, false) => EvidenceCell::Present,
        (false, true) => EvidenceCell::Absent,
        _ => EvidenceCell::Unmeasured,
    }
}
