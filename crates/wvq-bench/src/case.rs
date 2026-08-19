//! Shadow-case inputs. Human-touch time is optional and required for any 10× talk.

use serde::Serialize;
use wvq_intelligence::{ObligationNeed, TestCandidate};

/// v1 ecosystems the harness must evaluate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ecosystem {
    /// TypeScript / frontend (Vitest, component/story).
    TsFrontend,
    /// Node or Bun backend.
    NodeBunBackend,
    /// Go service.
    GoService,
}

/// A labelled quality finding. Expected vs observed is the FP/FN source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindingLabel {
    /// Stable finding identity (`WVQ-DEAD-001:src/x.ts`).
    pub id: String,
    /// Ground truth: the finding should fire.
    pub expected: bool,
    /// Whether the current gates actually fired.
    pub observed: bool,
}

/// A known human bug that some test is able to recover.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KnownBug {
    /// Bug identity.
    pub id: String,
    /// Test id that recovers it when selected.
    pub recovering_test: String,
}

/// One repository/change snapshot for shadow evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowCase {
    /// Case name (`sankey-others`, `go-add`, …).
    pub name: String,
    /// Which v1 ecosystem this case represents.
    pub ecosystem: Ecosystem,
    /// Full candidate set (the suite that would run without selection).
    pub candidates: Vec<TestCandidate>,
    /// Obligations that must be covered.
    pub obligations: Vec<ObligationNeed>,
    /// Human-confirmed bugs in this change.
    pub bugs: Vec<KnownBug>,
    /// Labelled findings for FP/FN.
    pub findings: Vec<FindingLabel>,
    /// Planning tokens actually spent compiling this change.
    pub planning_tokens: u64,
    /// Runtime LLM tokens. Green path must be `0`.
    pub runtime_tokens: u64,
    /// Stored artifact bytes (CAS), not context dumps.
    pub artifact_bytes: u64,
    /// Minutes a human spent on this change. Required before any 10× claim.
    pub human_touch_minutes: Option<u64>,
    /// Baseline human minutes (pre-WVQ) for the same class of change.
    pub baseline_human_touch_minutes: Option<u64>,
    /// Escaped regressions vs the previous protection set. Must not increase.
    pub escaped_regressions_delta: i64,
}

impl ShadowCase {
    /// Count findings that fired without ground truth.
    #[must_use]
    pub fn false_positives(&self) -> u64 {
        self.findings
            .iter()
            .filter(|item| item.observed && !item.expected)
            .count() as u64
    }

    /// Count findings that should have fired and did not.
    #[must_use]
    pub fn false_negatives(&self) -> u64 {
        self.findings
            .iter()
            .filter(|item| item.expected && !item.observed)
            .count() as u64
    }
}

/// Why a 10× publication is refused. `None` means the *gate* would allow it;
/// the harness still does not print a 10× headline.
#[must_use]
pub fn ten_x_publication_blocked_reason(case: &ShadowCase) -> Option<&'static str> {
    if case.human_touch_minutes.is_none() {
        return Some("human-touch-time data is missing");
    }
    if case.baseline_human_touch_minutes.is_none() {
        return Some("no baseline human-touch-time for a ratio");
    }
    if case.escaped_regressions_delta > 0 {
        return Some("escaped regressions increased");
    }
    None
}
