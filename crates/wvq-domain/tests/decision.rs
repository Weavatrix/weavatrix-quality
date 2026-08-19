//! Task 23: a human decision names one reviewer and one subject.

use wvq_domain::{
    ContentHash, DecisionError, HumanDecision, HumanDecisionId, HumanRole, NewDecision,
    VerificationDecision,
};

fn digest() -> ContentHash {
    ContentHash::new("ab".repeat(32)).unwrap()
}

fn build(reviewer: &str, subject: &str) -> Result<HumanDecision, DecisionError> {
    HumanDecision::new(NewDecision {
        id: HumanDecisionId::new("hd-1").unwrap(),
        reviewer: reviewer.to_owned(),
        role: HumanRole::Qa,
        subject: subject.to_owned(),
        artifact_digest: digest(),
        decision: VerificationDecision::AcceptAsIntended,
        comment: None,
        decided_at: "2026-08-20T09:00:00Z".to_owned(),
    })
}

#[test]
fn bulk_subjects_are_refused() {
    for subject in ["", "   ", "*", "all", "ALL", "any", "a,b", "others visible"] {
        assert_eq!(
            build("sergii", subject).unwrap_err(),
            DecisionError::BulkSubject,
            "subject {subject:?} must not approve more than one thing"
        );
    }
    assert!(build("sergii", "others-visible").is_ok());
}

#[test]
fn an_anonymous_reviewer_is_refused() {
    assert_eq!(
        build("  ", "others-visible").unwrap_err(),
        DecisionError::MissingReviewer
    );
}

#[test]
fn observed_only_is_never_normative_and_escalations_block_sealing() {
    assert!(!VerificationDecision::ObservedOnly.seal_eligible());
    assert!(!VerificationDecision::Reject.seal_eligible());
    assert!(VerificationDecision::AcceptAsIntended.seal_eligible());

    assert!(VerificationDecision::RequestProductDecision.escalates());
    assert!(VerificationDecision::RequestDeveloperClarification.escalates());
    assert!(!VerificationDecision::RequestProductDecision.seal_eligible());
    assert!(!VerificationDecision::AcceptAsIntended.escalates());
}

#[test]
fn decision_tokens_are_stable() {
    assert_eq!(HumanRole::Qa.as_str(), "qa");
    assert_eq!(HumanRole::Product.as_str(), "product");
    assert_eq!(
        VerificationDecision::MarkNonBehavioral.as_str(),
        "mark_non_behavioral"
    );
}
