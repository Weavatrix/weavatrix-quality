//! Task 28: the reviewed recovery workflow, end to end.

use wvq_domain::{
    ContentHash, HumanDecision, HumanDecisionId, HumanRole, NewDecision, VerificationDecision,
};
use wvq_spec_recovery::{
    CandidateRequirement, CandidateShape, CandidateState, CommitFacts, EvidenceSource,
    IntentEvidence, NarrativeInput, PublicSurfaceDelta, RecoveryDesk, RecoveryInput, ReviewError,
    TestIntentSummary, VerifyContext, cluster, narrate,
};

const DECLARED: &str = "cand-others-visible";
const RECOVERED: &str = "cand-viewer-denied";

fn declared_candidate() -> CandidateRequirement {
    CandidateRequirement {
        id: DECLARED.into(),
        subject: "sankey.others-node".into(),
        text: "Overflow values SHALL be represented by an Others node.".into(),
        expected_to_hold: true,
        actor: Some("viewer".into()),
        precondition: Some("cardinality exceeds the visual limit".into()),
        trigger: Some("open the dashboard".into()),
        evidence: vec![IntentEvidence::new(
            EvidenceSource::AcceptanceCriterion,
            "Overflow values SHALL be represented by an Others node.",
            "openspec/changes/sankey-others/specs/sankey/spec.md:12",
        )],
        ..CandidateRequirement::default()
    }
}

/// Recovered purely from a removed permission test: weak oracle, needs product.
fn recovered_candidate() -> CandidateRequirement {
    CandidateRequirement {
        id: RECOVERED.into(),
        subject: "sankey.viewer-denied".into(),
        text: "Viewer SHALL remain unable to open the details.".into(),
        expected_to_hold: true,
        actor: Some("viewer".into()),
        precondition: Some("viewer is signed in".into()),
        trigger: Some("activate Others".into()),
        evidence: vec![
            IntentEvidence::new(EvidenceSource::CodeDelta, "guard changed", "src/auth.ts"),
            IntentEvidence::new(
                EvidenceSource::ChangedTest,
                "viewer-deny test removed",
                "tests/auth-viewer.spec.ts",
            ),
        ],
        shape: CandidateShape {
            permission_sensitive: true,
            ..CandidateShape::default()
        },
        covered_cases: vec!["denied".into()],
        ..CandidateRequirement::default()
    }
}

fn input(candidates: Vec<CandidateRequirement>) -> RecoveryInput {
    let clusters = cluster(&[CommitFacts {
        id: "c1".into(),
        title: "add others endpoint".into(),
        index: 0,
        capability: Some("sankey".into()),
        ..CommitFacts::default()
    }]);
    let narrative = narrate(NarrativeInput {
        change_cluster: "sankey-others".into(),
        base_revision: "rev-base".into(),
        head_revision: "rev-head".into(),
        evidence: candidates
            .iter()
            .flat_map(|item| item.evidence.clone())
            .collect(),
        ..NarrativeInput::default()
    });
    RecoveryInput {
        narrative,
        clusters,
        surface_delta: PublicSurfaceDelta {
            added: vec!["GET /api/sankey/others".into()],
            removed: Vec::new(),
        },
        test_intent: vec![TestIntentSummary {
            test: "tests/sankey-others.spec.ts".into(),
            appears_to_expect: "an Others node above the limit".into(),
            changed_with_implementation: true,
        }],
        candidates,
        context: VerifyContext::default(),
    }
}

fn decision(
    id: &str,
    subject: &str,
    role: HumanRole,
    verdict: VerificationDecision,
    digest: &str,
) -> HumanDecision {
    HumanDecision::new(NewDecision {
        id: HumanDecisionId::new(id).unwrap(),
        reviewer: "sergii".into(),
        role,
        subject: subject.into(),
        artifact_digest: ContentHash::new(digest).unwrap(),
        decision: verdict,
        comment: None,
        decided_at: "2026-08-20T09:00:00Z".into(),
    })
    .unwrap()
}

fn digest_of(desk: &RecoveryDesk, candidate: &str) -> String {
    desk.review()
        .candidates
        .into_iter()
        .find(|item| item.id == candidate)
        .expect("candidate is on the review screen")
        .digest
}

#[test]
fn recovering_builds_a_packet_and_opens_reviews() {
    let mut desk = RecoveryDesk::new("sankey-others");
    let packet = desk.recover(input(vec![declared_candidate(), recovered_candidate()]));
    assert_eq!(packet.base_revision, "rev-base");
    assert_eq!(packet.head_revision, "rev-head");
    assert_eq!(packet.capability_clusters.len(), 1);
    assert_eq!(packet.public_surface_delta.added.len(), 1);
    assert!(
        packet
            .quality_heuristics
            .iter()
            .any(|item| item.contains("test changed with implementation")),
        "a test changed alongside the implementation is a known oracle risk"
    );

    assert_eq!(desk.state_of(DECLARED), Some(CandidateState::QaReview));
    assert_eq!(desk.state_of(RECOVERED), Some(CandidateState::QaReview));
}

#[test]
fn a_weak_oracle_is_escalated_to_product_in_the_questions() {
    let mut desk = RecoveryDesk::new("sankey-others");
    desk.recover(input(vec![declared_candidate(), recovered_candidate()]));

    let questions = desk.questions();
    assert!(questions.needs_product());
    assert!(
        questions
            .for_product
            .iter()
            .any(|item| item.starts_with(RECOVERED)),
        "the implementation-only candidate is the one that must escalate"
    );
    assert!(
        !questions
            .for_product
            .iter()
            .any(|item| item.starts_with(DECLARED)),
        "a declared criterion does not need a product decision"
    );
    assert!(
        questions
            .for_qa
            .iter()
            .any(|item| item.contains("Admin, Operator and Viewer")),
        "a permission surface asks the permission checklist"
    );
}

#[test]
fn the_patch_preview_is_always_a_proposal() {
    let mut desk = RecoveryDesk::new("sankey-others");
    desk.recover(input(vec![declared_candidate(), recovered_candidate()]));
    let patch = desk.preview_patch();
    assert!(patch.starts_with("# PROPOSED — not approved, not sealed"));
    assert!(patch.contains("### Requirement cand-others-visible"));
    assert!(patch.contains("[A] Overflow values SHALL"), "{patch}");
    assert!(patch.contains("## Escalated to product"));
}

#[test]
fn qa_verification_seals_a_declared_candidate() {
    let mut desk = RecoveryDesk::new("sankey-others");
    desk.recover(input(vec![declared_candidate()]));
    let digest = digest_of(&desk, DECLARED);

    let state = desk
        .decide(&decision(
            "hd-1",
            DECLARED,
            HumanRole::Qa,
            VerificationDecision::AcceptAsIntended,
            &digest,
        ))
        .unwrap();
    assert_eq!(state, CandidateState::QaVerified);

    let approval = desk.seal(DECLARED).unwrap();
    assert_eq!(approval.qa_verification.as_str(), "hd-1");
    assert_eq!(desk.state_of(DECLARED), Some(CandidateState::Sealed));
}

#[test]
fn an_implementation_only_candidate_needs_qa_and_product() {
    let mut desk = RecoveryDesk::new("sankey-others");
    desk.recover(input(vec![recovered_candidate()]));
    let digest = digest_of(&desk, RECOVERED);

    desk.decide(&decision(
        "hd-2",
        RECOVERED,
        HumanRole::Qa,
        VerificationDecision::AcceptAsIntended,
        &digest,
    ))
    .unwrap();
    assert_eq!(
        desk.seal(RECOVERED).unwrap_err(),
        ReviewError::ProductApprovalRequired,
        "behaviour reconstructed only from code cannot seal on QA alone"
    );
}

#[test]
fn nothing_seals_without_a_human() {
    let mut desk = RecoveryDesk::new("sankey-others");
    desk.recover(input(vec![declared_candidate()]));
    assert_eq!(
        desk.seal(DECLARED).unwrap_err(),
        ReviewError::QaVerificationMissing
    );
    assert_eq!(desk.state_of(DECLARED), Some(CandidateState::QaReview));
}

#[test]
fn an_unknown_candidate_is_refused() {
    let mut desk = RecoveryDesk::new("sankey-others");
    desk.recover(input(vec![declared_candidate()]));
    assert!(matches!(
        desk.seal("cand-does-not-exist").unwrap_err(),
        ReviewError::SubjectMismatch { .. }
    ));
}
