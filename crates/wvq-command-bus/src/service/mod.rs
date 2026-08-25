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

mod authoring;
mod recovery;
mod policy;
mod delta;
mod types;
mod error;
mod api;
mod fake;
mod live;
mod validate;
mod git;
mod graph;
mod paths;
mod verify_reply;
mod verify_axes;
mod verify_json;
mod verify_debt;
mod selection_build;
mod selection_audit;
mod execute;
mod persist_run;
mod persist_browser;
mod persist_failure_reel;
mod persist_ui;
mod persist_ui_analyse;
mod persist_behavior;
mod persist_evidence;
mod impact;
mod protection_snapshot;
mod protection_coverage;
mod protection_view;
mod protection_lineage;
mod protection_graph_extra;
mod analytics;
mod runner;
mod runner_coverage;
mod access;

pub use api::{QualityService, dispatch};
pub use error::BusError;
pub use fake::FakeService;
pub use live::LiveService;

pub(in crate::service) use authoring::*;
pub(in crate::service) use types::*;
pub(in crate::service) use git::*;
pub(in crate::service) use graph::*;
pub(in crate::service) use paths::*;
pub(in crate::service) use persist_run::*;
pub(in crate::service) use persist_evidence::*;
pub(in crate::service) use protection_snapshot::*;

#[cfg(test)]
pub(in crate::service) use std::collections::BTreeSet;
#[cfg(test)]
pub(in crate::service) use std::path::Path;
#[cfg(test)]
pub(in crate::service) use serde_json::{Value, json};
#[cfg(test)]
pub(in crate::service) use wvq_runtime::{CaptureWhen, CoverageArtifact, ExecutorTarget, TestStatus};
#[cfg(test)]
pub(in crate::service) use policy::*;
#[cfg(test)]
pub(in crate::service) use delta::*;
#[cfg(test)]
pub(in crate::service) use selection_build::*;
#[cfg(test)]
pub(in crate::service) use selection_audit::*;
#[cfg(test)]
pub(in crate::service) use execute::*;
#[cfg(test)]
pub(in crate::service) use persist_ui::*;
#[cfg(test)]
pub(in crate::service) use impact::*;
#[cfg(test)]
pub(in crate::service) use protection_coverage::*;
#[cfg(test)]
pub(in crate::service) use protection_lineage::*;
#[cfg(test)]
pub(in crate::service) use analytics::*;
#[cfg(test)]
pub(in crate::service) use runner::*;

#[cfg(test)]
mod tests;
