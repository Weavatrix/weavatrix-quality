//! Observe-only calibration: compare WVQ findings with QA outcomes.
//!
//! Stage A of the CI rollout. WVQ may record that it would have blocked, but
//! this report never becomes a gate. Unmeasured is not a clean pass.

use serde::{Deserialize, Serialize};

/// Campaign ceiling from the spec (30–50 historical/current PRs).
pub const MAX_CALIBRATION_CASES: usize = 50;

/// What QA or the merge record decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QaOutcome {
    /// Merged; no defect was attributed to the change.
    MergedClean,
    /// QA or review rejected the change.
    Rejected,
    /// A defect was found that this change owned.
    DefectFound,
}

/// What WVQ would have done if it had been a gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WvqObservation {
    /// Composite verdict would fail CI.
    WouldBlock,
    /// Composite verdict would not fail CI.
    WouldNotBlock,
    /// Required evidence was missing. Not a pass and not a failure.
    Unmeasured,
}

/// One labelled PR or change in a Stage A campaign.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalibrationCase {
    /// PR or change identity, e.g. `pr:12`.
    pub change: String,
    /// QA / merge outcome.
    pub qa: QaOutcome,
    /// WVQ observation for that same change.
    pub wvq: WvqObservation,
}

/// Confusion counts. [`Self::blocks_ci`] is always false.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalibrationReport {
    /// Cases actually scored (≤ [`MAX_CALIBRATION_CASES`]).
    pub cases: u64,
    /// True when the input exceeded the campaign ceiling.
    pub truncated: bool,
    /// Cases with a measured WVQ observation.
    pub measured: u64,
    /// Cases WVQ did not measure. Not true negatives.
    pub unmeasured: u64,
    /// WVQ would block and QA rejected or found a defect.
    pub true_positive: u64,
    /// WVQ would block and QA merged clean.
    pub false_positive: u64,
    /// WVQ would not block and QA rejected or found a defect.
    pub false_negative: u64,
    /// WVQ would not block and QA merged clean.
    pub true_negative: u64,
    /// Stage A never fails CI.
    pub blocks_ci: bool,
}

impl CalibrationReport {
    /// `true_positive / (true_positive + false_positive)` as counts, if defined.
    #[must_use]
    pub fn precision(&self) -> Option<(u64, u64)> {
        let denom = self.true_positive.saturating_add(self.false_positive);
        (denom > 0).then_some((self.true_positive, denom))
    }

    /// `true_positive / (true_positive + false_negative)` as counts, if defined.
    #[must_use]
    pub fn recall(&self) -> Option<(u64, u64)> {
        let denom = self.true_positive.saturating_add(self.false_negative);
        (denom > 0).then_some((self.true_positive, denom))
    }
}

/// Score a labelled campaign. Extra cases beyond the ceiling are dropped.
#[must_use]
pub fn calibrate_observe_only(cases: &[CalibrationCase]) -> CalibrationReport {
    let truncated = cases.len() > MAX_CALIBRATION_CASES;
    let scored = &cases[..cases.len().min(MAX_CALIBRATION_CASES)];
    let mut report = CalibrationReport {
        cases: scored.len() as u64,
        truncated,
        measured: 0,
        unmeasured: 0,
        true_positive: 0,
        false_positive: 0,
        false_negative: 0,
        true_negative: 0,
        blocks_ci: false,
    };
    for case in scored {
        match case.wvq {
            WvqObservation::Unmeasured => {
                report.unmeasured = report.unmeasured.saturating_add(1);
            }
            WvqObservation::WouldBlock => {
                report.measured = report.measured.saturating_add(1);
                if qa_bad(case.qa) {
                    report.true_positive = report.true_positive.saturating_add(1);
                } else {
                    report.false_positive = report.false_positive.saturating_add(1);
                }
            }
            WvqObservation::WouldNotBlock => {
                report.measured = report.measured.saturating_add(1);
                if qa_bad(case.qa) {
                    report.false_negative = report.false_negative.saturating_add(1);
                } else {
                    report.true_negative = report.true_negative.saturating_add(1);
                }
            }
        }
    }
    report
}

fn qa_bad(outcome: QaOutcome) -> bool {
    matches!(outcome, QaOutcome::Rejected | QaOutcome::DefectFound)
}
