//! Weavatrix-backed code evidence for Weavatrix Quality.
//!
//! This crate does **not** parse source and does **not** build a second code
//! graph. [`weavatrix_rust`] is the only repository/code authority. WVQ stores
//! revision-bound references (`repository` + `revision` + counts), never a
//! duplicate `Graph`.

#![forbid(unsafe_code)]

mod checks;
mod debt;
mod flow;
mod impact_union;
mod risk;
mod selection;
mod weavatrix;

pub use checks::{
    CoverageMeasurement, NodeCoverage, gate_api, gate_architecture, gate_clones, gate_coverage,
    gate_dead_code, gate_history, gate_topology, map_architecture_report,
    map_architecture_violation, map_coverage_to_nodes,
};
pub use flow::{
    FlowEntry, FlowFingerprint, FlowMatch, FlowState, ImpactedFlow, fingerprint, match_flows,
};
pub use impact_union::{GraphDelta, ImpactedSurface, SurfaceDelta, impacted_surface};
pub use risk::{RiskEvidence, RiskEvidenceKind, RiskLevel, risk_evidence};
pub use selection::{
    ObligationNeed, SelectedTest, SelectionInput, SelectionPlan, TestCandidate, select_minimal_plan,
};
pub use debt::{DebtBaseline, DebtDelta, DebtException, classify_debt};
pub use weavatrix::{
    CodeEvidenceProvider, IntelligenceError, RepoEvidence, WeavatrixProvider,
};
