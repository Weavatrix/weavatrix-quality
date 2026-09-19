//! Domain facade. CLI and MCP call this; they do not reimplement policy.
//!
//! Helper modules import shared names through [`access`]. Tests import the
//! same names from this module. Glob re-exports are the split, not a second
//! policy layer.
#![allow(clippy::wildcard_imports, clippy::doc_markdown)]

/// CAS artifact kind holding the base/head UI-integrity ratchet for one run.
pub(in crate::service) const UI_INTEGRITY_DELTA_KIND: &str = "ui-integrity-delta";
/// CAS artifact kind for changed-region source mutation evidence.
pub(in crate::service) const MUTATION_RESULTS_KIND: &str = "mutation-results";
/// CAS artifact kind holding live same-program Spec x Code x Behavior evidence.
pub(in crate::service) const DELTA_TRIANGLE_KIND: &str = "delta-triangle";
/// CAS artifact kind for the exact expectation replacement a QA reviewed.
pub(in crate::service) const ORACLE_REPLACEMENT_KIND: &str = "oracle-replacement-proposal";
/// CAS artifact kind for the read-only Application Surface Graph projection.
pub(in crate::service) const APPLICATION_SURFACE_GRAPH_KIND: &str = "application-surface-graph";
/// CAS artifact kind for evidenced Role/State/Action/Flag combinations.
pub(in crate::service) const BEHAVIOR_SURFACE_GRAPH_KIND: &str = "behavior-surface-graph";
/// CAS artifact kind for the read-only Surface Evidence Matrix.
pub(in crate::service) const SURFACE_EVIDENCE_MATRIX_KIND: &str = "surface-evidence-matrix";
/// CAS artifact kind for the read-only cheapest-evidence plan.
pub(in crate::service) const CHEAPEST_EVIDENCE_PLAN_KIND: &str = "cheapest-evidence-plan";
/// CAS artifact kind for an admitted continuous observation journal.
pub(in crate::service) const CONTINUOUS_OBSERVATION_JOURNAL_KIND: &str =
    "continuous-observation-journal";
/// CAS artifact kind for a HAR-derived privacy-safe network cassette.
pub(in crate::service) const NETWORK_CASSETTE_KIND: &str = "network-replay-profile";

mod access;
mod analytics;
mod api;
mod authoring;
mod delta;
mod error;
mod execute;
mod fake;
mod git;
mod graph;
mod impact;
mod live;
mod paths;
mod persist_behavior;
mod persist_browser;
mod persist_evidence;
mod persist_failure_reel;
mod persist_matrix;
mod persist_plan;
mod persist_run;
mod persist_surface;
mod persist_ui;
mod persist_ui_analyse;
mod policy;
mod protection_coverage;
mod protection_graph_extra;
mod protection_lineage;
mod protection_snapshot;
mod protection_view;
mod publish_coverage;
mod recovery;
mod runner;
mod runner_coverage;
mod runtime_profile;
mod selection_audit;
mod selection_build;
mod types;
mod ui_plan;
mod validate;
mod verify_axes;
mod verify_debt;
mod verify_json;
mod verify_reply;

pub use api::{QualityService, dispatch};
pub use error::BusError;
pub use fake::FakeService;
pub use live::LiveService;

pub(in crate::service) use authoring::*;
pub(in crate::service) use git::*;
pub(in crate::service) use graph::*;
pub(in crate::service) use paths::*;
pub(in crate::service) use persist_evidence::*;
pub(in crate::service) use persist_run::*;
pub(in crate::service) use protection_snapshot::*;
pub(in crate::service) use types::*;

#[cfg(test)]
pub(in crate::service) use analytics::*;
#[cfg(test)]
pub(in crate::service) use delta::*;
#[cfg(test)]
pub(in crate::service) use execute::*;
#[cfg(test)]
pub(in crate::service) use impact::*;
#[cfg(test)]
pub(in crate::service) use persist_matrix::*;
#[cfg(test)]
pub(in crate::service) use persist_plan::*;
#[cfg(test)]
pub(in crate::service) use persist_surface::*;
#[cfg(test)]
pub(in crate::service) use persist_ui::*;
#[cfg(test)]
pub(in crate::service) use policy::*;
#[cfg(test)]
pub(in crate::service) use protection_coverage::*;
#[cfg(test)]
pub(in crate::service) use protection_lineage::*;
#[cfg(test)]
pub(in crate::service) use publish_coverage::*;
#[cfg(test)]
pub(in crate::service) use runner::*;
#[cfg(test)]
pub(in crate::service) use selection_audit::*;
#[cfg(test)]
pub(in crate::service) use selection_build::*;
#[cfg(test)]
pub(in crate::service) use serde_json::{Value, json};
#[cfg(test)]
pub(in crate::service) use std::collections::BTreeSet;
#[cfg(test)]
pub(in crate::service) use std::path::Path;
#[cfg(test)]
pub(in crate::service) use wvq_runtime::{
    CaptureWhen, CoverageArtifact, ExecutorTarget, TestStatus,
};

#[cfg(test)]
mod tests;
