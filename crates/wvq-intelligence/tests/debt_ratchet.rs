//! Task 5: no-new-debt ratchet — existing / new / fixed / returned.

use wvq_domain::{CheckId, FindingState, QualityFinding, Severity, SubjectRef};
use wvq_intelligence::{DebtBaseline, DebtException, classify_debt};

fn finding(check: &str, file: &str, summary: &str) -> QualityFinding {
    QualityFinding::new(
        CheckId::new(check).unwrap(),
        Severity::Warn,
        SubjectRef::File(file.into()),
        summary,
    )
}

#[test]
fn classifies_existing_new_fixed_and_returned() {
    let dead = finding("WVQ-DEAD-001", "src/legacy.js", "dead helper");
    let clone = finding("WVQ-CLONE-001", "src/dup.js", "new clone family");
    let cycle = finding("WVQ-ARCH-003", "src/cycle.js", "runtime cycle");
    let returned = finding("WVQ-SIZE-002", "src/big.js", "oversized file grew");

    let mut baseline = DebtBaseline::default();
    baseline
        .previously_fixed
        .insert(returned.fingerprint());

    let delta = classify_debt(
        &[dead.clone(), cycle.clone()],
        &[dead.clone(), clone.clone(), returned.clone()],
        &baseline,
    );

    assert_eq!(ids(&delta.existing), ["src/legacy.js"]);
    assert_eq!(delta.existing[0].state, FindingState::Existing);
    assert_eq!(ids(&delta.new), ["src/dup.js"]);
    assert_eq!(delta.new[0].state, FindingState::New);
    assert_eq!(ids(&delta.fixed), ["src/cycle.js"]);
    assert_eq!(delta.fixed[0].state, FindingState::Fixed);
    assert_eq!(ids(&delta.returned), ["src/big.js"]);
    assert_eq!(delta.returned[0].state, FindingState::Returned);
}

#[test]
fn fingerprint_is_independent_of_input_order_and_summary() {
    let a = finding("WVQ-DEAD-001", "src/a.js", "first wording");
    let b = finding("WVQ-DEAD-002", "src/b.js", "keep");
    let a_reworded = finding("WVQ-DEAD-001", r"src\a.js", "different wording");

    let left = classify_debt(&[a.clone(), b.clone()], std::slice::from_ref(&a_reworded), &DebtBaseline::default());
    let right = classify_debt(&[b, a], std::slice::from_ref(&a_reworded), &DebtBaseline::default());

    assert_eq!(left, right);
    assert_eq!(left.existing.len(), 1);
    assert_eq!(left.fixed.len(), 1);
    assert_eq!(left.existing[0].fingerprint().subject_value, "src/a.js");
    assert_eq!(
        left.existing[0].fingerprint().check.as_str(),
        "WVQ-DEAD-001"
    );
}

#[test]
fn previously_fixed_fingerprint_returning_is_returned_not_new() {
    let item = finding("WVQ-ARCH-001", "src/layer.js", "layer bypass");
    let mut baseline = DebtBaseline::default();
    baseline.previously_fixed.insert(item.fingerprint());

    let first = classify_debt(std::slice::from_ref(&item), &[], &DebtBaseline::default());
    assert_eq!(first.fixed[0].state, FindingState::Fixed);

    let again = classify_debt(&[], std::slice::from_ref(&item), &baseline);
    assert!(again.new.is_empty());
    assert_eq!(again.returned.len(), 1);
    assert_eq!(again.returned[0].state, FindingState::Returned);
}

#[test]
fn excepted_head_finding_is_not_new_debt() {
    let item = finding("WVQ-GRAPH-001", "src/god.js", "god node");
    let mut baseline = DebtBaseline::default();
    baseline.excepted.insert(
        item.fingerprint(),
        DebtException {
            reason: "tracked in Q3 cleanup".into(),
            expires: Some("2026-12-01".into()),
        },
    );

    let delta = classify_debt(&[], std::slice::from_ref(&item), &baseline);
    assert!(delta.new.is_empty());
    assert_eq!(delta.excepted.len(), 1);
    assert_eq!(delta.excepted[0].state, FindingState::Excepted);
}

fn ids(findings: &[QualityFinding]) -> Vec<&str> {
    findings
        .iter()
        .map(|finding| match &finding.subject {
            SubjectRef::File(path) => path.as_str(),
            _ => "?",
        })
        .collect()
}
