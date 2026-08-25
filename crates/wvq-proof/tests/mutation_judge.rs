//! Known flaky judges cannot independently produce an authoritative kill.

use wvq_proof::{MutantStatus, authoritative_mutant_status};

#[test]
fn a_stable_failure_kills() {
    assert_eq!(
        authoritative_mutant_status(true, false),
        MutantStatus::Killed
    );
}

#[test]
fn a_stable_pass_survives_even_when_a_flaky_judge_also_failed() {
    let flaky_failed = true;
    let stable_failed = false;
    let stable_passed = true;
    assert!(flaky_failed);
    assert_eq!(
        authoritative_mutant_status(stable_failed, stable_passed),
        MutantStatus::Survived
    );
}

#[test]
fn a_flaky_failure_without_a_stable_judge_is_invalid_not_killed() {
    let flaky_failed = true;
    let stable_failed = false;
    let stable_passed = false;
    assert!(flaky_failed);
    assert_eq!(
        authoritative_mutant_status(stable_failed, stable_passed),
        MutantStatus::Invalid
    );
}

#[test]
fn no_judges_is_invalid() {
    assert_eq!(
        authoritative_mutant_status(false, false),
        MutantStatus::Invalid
    );
}
