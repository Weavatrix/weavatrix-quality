//! Observe-only calibration: unmeasured is not a clean pass, and Stage A never gates.

use wvq_proof::{
    CalibrationCase, QaOutcome, WvqObservation, calibrate_observe_only, MAX_CALIBRATION_CASES,
};

fn case(change: &str, qa: QaOutcome, wvq: WvqObservation) -> CalibrationCase {
    CalibrationCase {
        change: change.into(),
        qa,
        wvq,
    }
}

#[test]
fn unmeasured_merged_prs_are_not_true_negatives() {
    let report = calibrate_observe_only(&[
        case("pr:1", QaOutcome::MergedClean, WvqObservation::Unmeasured),
        case("pr:2", QaOutcome::MergedClean, WvqObservation::Unmeasured),
    ]);
    assert_eq!(report.unmeasured, 2);
    assert_eq!(report.true_negative, 0);
    assert_eq!(report.false_positive, 0);
    assert!(!report.blocks_ci);
    assert_eq!(report.precision(), None);
    assert_eq!(report.recall(), None);
}

#[test]
fn a_would_block_on_a_clean_merge_is_a_false_positive() {
    let report = calibrate_observe_only(&[case(
        "pr:3",
        QaOutcome::MergedClean,
        WvqObservation::WouldBlock,
    )]);
    assert_eq!(report.false_positive, 1);
    assert_eq!(report.true_positive, 0);
    assert_eq!(report.precision(), Some((0, 1)));
    assert!(!report.blocks_ci);
}

#[test]
fn a_would_block_on_a_qa_reject_is_a_true_positive() {
    let report = calibrate_observe_only(&[
        case("pr:4", QaOutcome::Rejected, WvqObservation::WouldBlock),
        case("pr:5", QaOutcome::DefectFound, WvqObservation::WouldBlock),
    ]);
    assert_eq!(report.true_positive, 2);
    assert_eq!(report.precision(), Some((2, 2)));
    assert_eq!(report.recall(), Some((2, 2)));
}

#[test]
fn a_clean_wvq_pass_that_qa_caught_is_a_false_negative() {
    let report = calibrate_observe_only(&[case(
        "pr:6",
        QaOutcome::DefectFound,
        WvqObservation::WouldNotBlock,
    )]);
    assert_eq!(report.false_negative, 1);
    assert_eq!(report.recall(), Some((0, 1)));
}

#[test]
fn a_measured_clean_agreement_is_a_true_negative() {
    let report = calibrate_observe_only(&[case(
        "pr:7",
        QaOutcome::MergedClean,
        WvqObservation::WouldNotBlock,
    )]);
    assert_eq!(report.true_negative, 1);
    assert_eq!(report.measured, 1);
}

#[test]
fn extra_cases_beyond_the_campaign_ceiling_are_truncated() {
    let cases: Vec<_> = (0..=MAX_CALIBRATION_CASES)
        .map(|index| {
            case(
                &format!("pr:{index}"),
                QaOutcome::MergedClean,
                WvqObservation::Unmeasured,
            )
        })
        .collect();
    let report = calibrate_observe_only(&cases);
    assert!(report.truncated);
    assert_eq!(report.cases, MAX_CALIBRATION_CASES as u64);
    assert_eq!(report.unmeasured, MAX_CALIBRATION_CASES as u64);
}

#[test]
fn a_mixed_campaign_keeps_unmeasured_out_of_precision() {
    let report = calibrate_observe_only(&[
        case("pr:10", QaOutcome::Rejected, WvqObservation::WouldBlock),
        case("pr:11", QaOutcome::MergedClean, WvqObservation::WouldBlock),
        case(
            "pr:12",
            QaOutcome::MergedClean,
            WvqObservation::Unmeasured,
        ),
        case(
            "pr:13",
            QaOutcome::MergedClean,
            WvqObservation::WouldNotBlock,
        ),
    ]);
    assert_eq!(report.true_positive, 1);
    assert_eq!(report.false_positive, 1);
    assert_eq!(report.true_negative, 1);
    assert_eq!(report.unmeasured, 1);
    assert_eq!(report.precision(), Some((1, 2)));
    assert!(!report.blocks_ci);
}
