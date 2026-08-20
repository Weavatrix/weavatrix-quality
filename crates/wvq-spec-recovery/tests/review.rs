//! Task 27: recovered intent reaches an oracle only through a human.

use wvq_domain::{
    ContentHash, HumanDecision, HumanDecisionId, HumanRole, NewDecision, VerificationDecision,
};
use wvq_spec_recovery::{
    CandidateRequirement, CandidateReview, CandidateState, EvidenceSource, IntentEvidence,
    ReviewError, VerifierReport, VerifyContext, verify_candidates,
};

const CANDIDATE: &str = "cand-others-visible";

fn digest(seed: &str) -> ContentHash {
    ContentHash::new(seed.repeat(32)).unwrap()
}

fn candidate(evidence: Vec<IntentEvidence>) -> CandidateRequirement {
    CandidateRequirement {
        id: CANDIDATE.into(),
        subject: "sankey.others-node".into(),
        text: "Overflow values SHALL be represented by an Others node.".into(),
        expected_to_hold: true,
        actor: Some("viewer".into()),
        precondition: Some("cardinality exceeds the visual limit".into()),
        trigger: Some("open the dashboard".into()),
        evidence,
        ..CandidateRequirement::default()
    }
}

fn declared_report() -> VerifierReport {
    verify_candidates(
        &[candidate(vec![IntentEvidence::new(
            EvidenceSource::AcceptanceCriterion,
            "Overflow values SHALL be represented by an Others node.",
            "openspec/.../spec.md:12",
        )])],
        &VerifyContext::default(),
    )
}

fn weak_oracle_report() -> VerifierReport {
    verify_candidates(
        &[candidate(vec![
            IntentEvidence::new(EvidenceSource::CodeDelta, "groupOverflow", "src/sankey.ts"),
            IntentEvidence::new(EvidenceSource::ChangedTest, "expects Others", "spec.ts"),
        ])],
        &VerifyContext::default(),
    )
}

fn decision(
    id: &str,
    role: HumanRole,
    verdict: VerificationDecision,
    seen: &ContentHash,
) -> HumanDecision {
    HumanDecision::new(NewDecision {
        id: HumanDecisionId::new(id).unwrap(),
        reviewer: "sergii".into(),
        role,
        subject: CANDIDATE.into(),
        artifact_digest: seen.clone(),
        decision: verdict,
        comment: None,
        decided_at: "2026-08-20T09:00:00Z".into(),
    })
    .unwrap()
}

/// Drive a review as far as `QA_REVIEW` with a declared-intent candidate.
fn awaiting_qa() -> CandidateReview {
    let mut review = CandidateReview::new(CANDIDATE, digest("ab"));
    review.propose().unwrap();
    review.auto_check(&declared_report()).unwrap();
    review.submit_for_review().unwrap();
    assert_eq!(review.state(), CandidateState::QaReview);
    review
}

#[test]
fn a_recovered_requirement_cannot_seal_without_qa() {
    let mut review = CandidateReview::new(CANDIDATE, digest("ab"));
    assert_eq!(review.state(), CandidateState::Recovered);
    assert_eq!(
        review.mark_seal_eligible().unwrap_err(),
        ReviewError::QaVerificationMissing
    );
    assert!(matches!(
        review.seal().unwrap_err(),
        ReviewError::IllegalTransition { .. }
    ));
    assert!(!review.state().is_normative());
}

#[test]
fn the_deterministic_checks_gate_the_review_queue() {
    let mut blocked = candidate(vec![IntentEvidence::new(
        EvidenceSource::AcceptanceCriterion,
        "x",
        "spec.md",
    )]);
    blocked.actor = None;
    let report = verify_candidates(&[blocked], &VerifyContext::default());

    let mut review = CandidateReview::new(CANDIDATE, digest("ab"));
    review.propose().unwrap();
    assert_eq!(
        review.auto_check(&report).unwrap_err(),
        ReviewError::ChecksBlockReview
    );
}

#[test]
fn qa_acceptance_reaches_a_seal() {
    let mut review = awaiting_qa();
    review
        .record_qa(&decision(
            "hd-1",
            HumanRole::Qa,
            VerificationDecision::AcceptAsIntended,
            &digest("ab"),
        ))
        .unwrap();
    assert_eq!(review.state(), CandidateState::QaVerified);
    review.mark_seal_eligible().unwrap();
    let approval = review.seal().unwrap();
    assert_eq!(approval.qa_verification.as_str(), "hd-1");
    assert_eq!(approval.product_approval, None);
    assert!(review.state().is_normative());
}

#[test]
fn observed_only_never_becomes_normative() {
    let mut review = awaiting_qa();
    review
        .record_qa(&decision(
            "hd-2",
            HumanRole::Qa,
            VerificationDecision::ObservedOnly,
            &digest("ab"),
        ))
        .unwrap();
    assert_eq!(review.state(), CandidateState::ObservedOnly);
    assert!(!review.state().is_normative());
    assert!(matches!(
        review.mark_seal_eligible().unwrap_err(),
        ReviewError::IllegalTransition { .. }
    ));
}

#[test]
fn requesting_a_product_decision_blocks_the_seal() {
    let mut review = awaiting_qa();
    review
        .record_qa(&decision(
            "hd-3",
            HumanRole::Qa,
            VerificationDecision::RequestProductDecision,
            &digest("ab"),
        ))
        .unwrap();
    assert_eq!(review.state(), CandidateState::ProductDecisionRequired);
    assert!(review.requires_product_approval());
    assert_eq!(
        review.mark_seal_eligible().unwrap_err(),
        ReviewError::ProductApprovalRequired
    );
}

#[test]
fn product_approval_resolves_the_escalation() {
    let mut review = awaiting_qa();
    review
        .record_qa(&decision(
            "hd-3",
            HumanRole::Qa,
            VerificationDecision::RequestProductDecision,
            &digest("ab"),
        ))
        .unwrap();
    review
        .record_product(&decision(
            "hd-4",
            HumanRole::Product,
            VerificationDecision::AcceptAsIntended,
            &digest("ab"),
        ))
        .unwrap();
    assert_eq!(review.state(), CandidateState::ProductApproved);
    review.mark_seal_eligible().unwrap();
    let approval = review.seal().unwrap();
    assert_eq!(approval.qa_verification.as_str(), "hd-3");
    assert_eq!(
        approval.product_approval.map(|id| id.as_str().to_owned()),
        Some("hd-4".to_owned())
    );
}

#[test]
fn a_weak_oracle_makes_product_approval_mandatory() {
    let mut review = CandidateReview::new(CANDIDATE, digest("ab"));
    review.propose().unwrap();
    review.auto_check(&weak_oracle_report()).unwrap();
    assert!(
        review.requires_product_approval(),
        "intent reconstructed only from implementation must escalate"
    );
    review.submit_for_review().unwrap();
    review
        .record_qa(&decision(
            "hd-5",
            HumanRole::Qa,
            VerificationDecision::AcceptAsIntended,
            &digest("ab"),
        ))
        .unwrap();
    assert_eq!(review.state(), CandidateState::QaVerified);
    assert_eq!(
        review.mark_seal_eligible().unwrap_err(),
        ReviewError::ProductApprovalRequired
    );
}

#[test]
fn an_edit_invalidates_a_decision_taken_against_the_old_digest() {
    let mut review = awaiting_qa();
    let stale = decision(
        "hd-6",
        HumanRole::Qa,
        VerificationDecision::AcceptAsIntended,
        &digest("ab"),
    );

    review.edit(digest("cd"));
    assert_eq!(review.state(), CandidateState::Proposed, "review restarts");
    review.auto_check(&declared_report()).unwrap();
    review.submit_for_review().unwrap();

    assert_eq!(
        review.record_qa(&stale).unwrap_err(),
        ReviewError::DigestChanged {
            seen: digest("ab").to_string(),
            current: digest("cd").to_string(),
        }
    );
    assert_eq!(
        review.mark_seal_eligible().unwrap_err(),
        ReviewError::QaVerificationMissing,
        "the earlier approval did not survive the edit"
    );
}

#[test]
fn a_decision_about_another_subject_or_role_is_refused() {
    let mut review = awaiting_qa();
    let other = HumanDecision::new(NewDecision {
        id: HumanDecisionId::new("hd-7").unwrap(),
        reviewer: "sergii".into(),
        role: HumanRole::Qa,
        subject: "some-other-candidate".into(),
        artifact_digest: digest("ab"),
        decision: VerificationDecision::AcceptAsIntended,
        comment: None,
        decided_at: "2026-08-20T09:00:00Z".into(),
    })
    .unwrap();
    assert!(matches!(
        review.record_qa(&other).unwrap_err(),
        ReviewError::SubjectMismatch { .. }
    ));

    let wrong_role = decision(
        "hd-8",
        HumanRole::Developer,
        VerificationDecision::AcceptAsIntended,
        &digest("ab"),
    );
    assert!(matches!(
        review.record_qa(&wrong_role).unwrap_err(),
        ReviewError::WrongRole { .. }
    ));
}

#[test]
fn rejection_is_terminal() {
    let mut review = awaiting_qa();
    review
        .record_qa(&decision(
            "hd-9",
            HumanRole::Qa,
            VerificationDecision::Reject,
            &digest("ab"),
        ))
        .unwrap();
    assert_eq!(review.state(), CandidateState::Rejected);
    assert!(matches!(
        review.mark_seal_eligible().unwrap_err(),
        ReviewError::IllegalTransition { .. }
    ));
}
