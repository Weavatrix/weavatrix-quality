//! Weavatrix-backed code evidence for Weavatrix Quality.
//!
//! This crate does **not** parse source and does **not** build a second code
//! graph. [`weavatrix_rust`] is the only repository/code authority. WVQ stores
//! revision-bound references (`repository` + `revision` + counts), never a
//! duplicate `Graph`.

#![forbid(unsafe_code)]

mod checks;
mod coverage_autopilot;
mod debt;
mod flow;
mod hypothesis;
mod impact_union;
mod risk;
mod selection;
mod surface_evidence;
mod surface_graph;
mod surface_reach;
mod test_lineage;
mod weavatrix;

pub use checks::{
    CoverageMeasurement, NodeCoverage, gate_api, gate_architecture, gate_clones, gate_coverage,
    gate_dead_code, gate_history, gate_topology, map_architecture_report,
    map_architecture_violation, map_coverage_to_nodes,
};
pub use coverage_autopilot::{
    CoverageAutopilot, SurfaceCoverage, SurfaceCoverageState, coverage_autopilot,
};
pub use surface_evidence::{
    EvidenceCell, MeasuredColumn, SurfaceEvidenceColumns, SurfaceEvidenceMatrix,
    SurfaceEvidenceRow, surface_evidence_matrix, surfaces_touching_nodes,
};
pub use surface_graph::{
    ApplicationSurface, ApplicationSurfaceGraph, ApplicationSurfaceKind, MAX_APPLICATION_SURFACES,
    SurfaceEvidenceKind, application_surface_graph,
};
pub use surface_reach::{BindingReach, production_nodes_for_binding};
pub use debt::{DebtBaseline, DebtDelta, DebtException, classify_debt};
pub use flow::{
    FlowEntry, FlowFingerprint, FlowMatch, FlowState, ImpactedFlow, fingerprint, match_flows,
};
pub use hypothesis::{
    ChangeSignal, DefectHypothesis, DetectedSignal, GraphFacts, HypothesisWeight, SignalConfidence,
    blocking_questions, corroborate, hypothesise,
};
pub use impact_union::{GraphDelta, ImpactedSurface, SurfaceDelta, impacted_surface};
pub use risk::{RiskEvidence, RiskEvidenceKind, RiskLevel, risk_evidence};
pub use selection::{
    CandidateSources, ObligationNeed, SelectedTest, SelectionInput, SelectionPlan, TestCandidate,
    flow_aware_candidates, select_flow_aware_plan, select_minimal_plan,
};
pub use test_lineage::{TestFacts, TestLineage, TestLineageState, track_lineage};
pub use weavatrix::{CodeEvidenceProvider, IntelligenceError, RepoEvidence, WeavatrixProvider};
