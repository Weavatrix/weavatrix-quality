//! Weavatrix-backed code evidence for Weavatrix Quality.
//!
//! This crate does **not** parse source and does **not** build a second code
//! graph. [`weavatrix_rust`] is the only repository/code authority. WVQ stores
//! revision-bound references (`repository` + `revision` + counts), never a
//! duplicate `Graph`.

#![forbid(unsafe_code)]

mod checks;
mod debt;
mod weavatrix;

pub use checks::{
    gate_architecture, gate_clones, gate_dead_code, gate_topology, map_architecture_report,
    map_architecture_violation,
};
pub use debt::{DebtBaseline, DebtDelta, DebtException, classify_debt};
pub use weavatrix::{
    CodeEvidenceProvider, IntelligenceError, RepoEvidence, WeavatrixProvider,
};
