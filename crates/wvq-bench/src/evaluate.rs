//! Compare greedy selection against the full suite. Never emit a 10× headline.

use std::collections::BTreeSet;

use serde::Serialize;
use wvq_intelligence::{SelectionInput, select_minimal_plan};

use crate::case::{ShadowCase, ten_x_publication_blocked_reason};

/// Measured shadow metrics for one case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShadowReport {
    /// Case name.
    pub name: String,
    /// Ecosystem token.
    pub ecosystem: String,
    /// Selection algorithm.
    pub algorithm: String,
    /// Tests selected.
    pub selected_count: u64,
    /// Full suite size.
    pub full_count: u64,
    /// Sum of selected effective costs (milliseconds or abstract units).
    pub selected_wall_clock_ms: u64,
    /// Sum of all candidate costs.
    pub full_wall_clock_ms: u64,
    /// Known bugs whose recovering test was selected.
    pub bugs_recovered: u64,
    /// Known bugs in the case.
    pub bugs_total: u64,
    /// Observed ∧ ¬expected.
    pub false_positives: u64,
    /// Expected ∧ ¬observed.
    pub false_negatives: u64,
    /// Planning tokens.
    pub planning_tokens: u64,
    /// Runtime LLM tokens.
    pub runtime_tokens: u64,
    /// Artifact bytes.
    pub artifact_bytes: u64,
    /// High-risk obligations the greedy plan could not cover.
    pub uncovered_mandatory: Vec<String>,
    /// Why a 10× claim must not be published, if any.
    pub ten_x_blocked: Option<String>,
}

impl ShadowReport {
    /// One-line machine-readable summary. Never contains a 10× claim.
    #[must_use]
    pub fn summary_line(&self) -> String {
        format!(
            "{} {}: selected {}/{} tests, {}/{} ms, bugs {}/{}, fp {}, fn {}, runtime_tokens {}, artifacts {}",
            self.ecosystem,
            self.name,
            self.selected_count,
            self.full_count,
            self.selected_wall_clock_ms,
            self.full_wall_clock_ms,
            self.bugs_recovered,
            self.bugs_total,
            self.false_positives,
            self.false_negatives,
            self.runtime_tokens,
            self.artifact_bytes
        )
    }
}

/// Run greedy selection and score it against the labelled case.
#[must_use]
pub fn evaluate(case: &ShadowCase) -> ShadowReport {
    let plan = select_minimal_plan(SelectionInput {
        candidates: case.candidates.clone(),
        obligations: case.obligations.clone(),
    });
    let selected_ids: BTreeSet<&str> = plan.selected.iter().map(|item| item.id.as_str()).collect();
    let selected_wall_clock_ms = plan.selected.iter().map(|item| item.cost).sum();
    let full_wall_clock_ms = case
        .candidates
        .iter()
        .map(|item| item.cost.saturating_add(item.flake_penalty))
        .sum();
    let bugs_recovered = case
        .bugs
        .iter()
        .filter(|bug| selected_ids.contains(bug.recovering_test.as_str()))
        .count() as u64;
    ShadowReport {
        name: case.name.clone(),
        ecosystem: ecosystem_token(case.ecosystem),
        algorithm: plan.algorithm.to_owned(),
        selected_count: plan.selected.len() as u64,
        full_count: case.candidates.len() as u64,
        selected_wall_clock_ms,
        full_wall_clock_ms,
        bugs_recovered,
        bugs_total: case.bugs.len() as u64,
        false_positives: case.false_positives(),
        false_negatives: case.false_negatives(),
        planning_tokens: case.planning_tokens,
        runtime_tokens: case.runtime_tokens,
        artifact_bytes: case.artifact_bytes,
        uncovered_mandatory: plan.uncovered_mandatory,
        ten_x_blocked: ten_x_publication_blocked_reason(case).map(ToOwned::to_owned),
    }
}

fn ecosystem_token(ecosystem: crate::Ecosystem) -> String {
    match ecosystem {
        crate::Ecosystem::TsFrontend => "ts_frontend".into(),
        crate::Ecosystem::NodeBunBackend => "node_bun_backend".into(),
        crate::Ecosystem::GoService => "go_service".into(),
    }
}
