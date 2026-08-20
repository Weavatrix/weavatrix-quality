//! Task 26: deterministic checks run before a human spends any review time.

use wvq_spec_recovery::{
    CandidateRequirement, CandidateShape, EvidenceSource, FindingKind, IntentEvidence,
    ObservedFact, VerifyContext, verify_candidates,
};

fn declared() -> Vec<IntentEvidence> {
    vec![IntentEvidence::new(
        EvidenceSource::AcceptanceCriterion,
        "Overflow values SHALL be represented by an Others node.",
        "openspec/changes/sankey-others/specs/sankey/spec.md:12",
    )]
}

fn implementation_only() -> Vec<IntentEvidence> {
    vec![
        IntentEvidence::new(
            EvidenceSource::CodeDelta,
            "groupOverflow added",
            "src/sankey.ts",
        ),
        IntentEvidence::new(
            EvidenceSource::ChangedTest,
            "expects Others",
            "tests/sankey.spec.ts",
        ),
    ]
}

/// A candidate that passes every structural check, so tests can vary one thing.
fn sound(id: &str, text: &str) -> CandidateRequirement {
    CandidateRequirement {
        id: id.into(),
        subject: "sankey.others-node".into(),
        text: text.into(),
        expected_to_hold: true,
        actor: Some("viewer".into()),
        precondition: Some("cardinality exceeds the visual limit".into()),
        trigger: Some("open the dashboard".into()),
        endpoint: None,
        evidence: declared(),
        shape: CandidateShape::default(),
        covered_cases: Vec::new(),
    }
}

#[test]
fn a_sound_candidate_produces_nothing() {
    let report = verify_candidates(
        &[sound(
            "cand-1",
            "Overflow values SHALL be represented by an Others node.",
        )],
        &VerifyContext::default(),
    );
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert!(!report.blocks_review());
}

#[test]
fn vague_wording_without_a_measurable_oracle_is_refused() {
    for text in [
        "The dashboard SHALL work correctly.",
        "The UI SHALL look good.",
        "The response SHALL be fast.",
    ] {
        let report = verify_candidates(&[sound("cand-1", text)], &VerifyContext::default());
        assert!(
            report.has("cand-1", FindingKind::NonTestableWording),
            "{text:?} must be refused"
        );
        assert!(report.blocks_review());
    }
}

#[test]
fn a_measurable_oracle_rescues_vague_wording() {
    let report = verify_candidates(
        &[sound(
            "cand-1",
            "The response SHALL be fast: within 200 ms at the 95th percentile.",
        )],
        &VerifyContext::default(),
    );
    assert!(!report.has("cand-1", FindingKind::NonTestableWording));
}

#[test]
fn implementation_leakage_is_refused() {
    for text in [
        "buildSankeyData() SHALL group the overflow.",
        "The node SHALL carry data-testid on the Others element.",
        "src/sankey.ts SHALL sort descending.",
    ] {
        let report = verify_candidates(&[sound("cand-1", text)], &VerifyContext::default());
        assert!(
            report.has("cand-1", FindingKind::ImplementationLeakage),
            "{text:?} names an implementation detail"
        );
    }
}

#[test]
fn a_missing_actor_precondition_or_trigger_is_reported() {
    let mut candidate = sound("cand-1", "Overflow values SHALL be grouped.");
    candidate.actor = None;
    candidate.precondition = None;
    candidate.trigger = None;
    let report = verify_candidates(&[candidate], &VerifyContext::default());
    assert!(report.has("cand-1", FindingKind::MissingActor));
    assert!(report.has("cand-1", FindingKind::MissingPrecondition));
    assert!(report.has("cand-1", FindingKind::MissingTrigger));
}

#[test]
fn a_numeric_limit_demands_below_at_and_above() {
    let mut candidate = sound("cand-1", "Above the visual limit values SHALL be grouped.");
    candidate.shape = CandidateShape {
        numeric_limit: true,
        ..CandidateShape::default()
    };
    candidate.covered_cases = vec!["above".into()];
    let report = verify_candidates(&[candidate.clone()], &VerifyContext::default());
    let boundary = report.of_kind(FindingKind::MissingBoundaryCase);
    assert_eq!(boundary.len(), 1);
    assert!(boundary[0].detail.contains("below"));
    assert!(boundary[0].detail.contains("at"));

    candidate.covered_cases = vec!["below".into(), "at".into(), "above".into()];
    let report = verify_candidates(&[candidate], &VerifyContext::default());
    assert!(!report.has("cand-1", FindingKind::MissingBoundaryCase));
}

#[test]
fn permission_and_async_surfaces_demand_a_negative_case() {
    let mut candidate = sound("cand-1", "An admin SHALL delete the report.");
    candidate.shape = CandidateShape {
        permission_sensitive: true,
        async_ui: true,
        ..CandidateShape::default()
    };
    let report = verify_candidates(&[candidate.clone()], &VerifyContext::default());
    assert_eq!(report.of_kind(FindingKind::MissingNegativeCase).len(), 2);

    candidate.covered_cases = vec!["denied".into(), "failure".into()];
    let report = verify_candidates(&[candidate], &VerifyContext::default());
    assert!(!report.has("cand-1", FindingKind::MissingNegativeCase));
}

#[test]
fn a_duplicate_of_an_active_requirement_is_reported() {
    let context = VerifyContext {
        existing_requirements: vec![
            "  Overflow values shall be represented by an Others node.  ".into(),
        ],
        ..VerifyContext::default()
    };
    let report = verify_candidates(
        &[sound(
            "cand-1",
            "Overflow values SHALL be represented by an Others node",
        )],
        &context,
    );
    assert!(report.has("cand-1", FindingKind::DuplicateRequirement));
}

#[test]
fn two_candidates_asserting_opposite_things_contradict() {
    let yes = sound("cand-1", "Viewer SHALL open the details.");
    let mut no = sound("cand-2", "Viewer SHALL NOT open the details.");
    no.expected_to_hold = false;
    let report = verify_candidates(&[yes, no], &VerifyContext::default());
    assert!(report.has("cand-1", FindingKind::InternalContradiction));
    assert!(report.has("cand-2", FindingKind::InternalContradiction));
}

#[test]
fn a_candidate_depending_on_a_removed_endpoint_contradicts_the_code() {
    let mut candidate = sound("cand-1", "The details SHALL be fetched.");
    candidate.endpoint = Some("GET /api/sankey/others".into());
    let context = VerifyContext {
        removed_endpoints: vec!["GET /api/sankey/others".into()],
        ..VerifyContext::default()
    };
    let report = verify_candidates(&[candidate], &context);
    assert!(report.has("cand-1", FindingKind::CodeContradiction));
}

#[test]
fn observed_behavior_disagreeing_with_intent_is_reported() {
    let context = VerifyContext {
        observed: vec![ObservedFact {
            subject: "sankey.others-node".into(),
            holds: false,
        }],
        ..VerifyContext::default()
    };
    let report = verify_candidates(&[sound("cand-1", "An Others node SHALL appear.")], &context);
    assert!(report.has("cand-1", FindingKind::BehaviorContradiction));
}

#[test]
fn a_weak_oracle_is_surfaced_without_blocking_review() {
    let mut candidate = sound("cand-1", "An Others node SHALL appear.");
    candidate.evidence = implementation_only();
    let report = verify_candidates(&[candidate], &VerifyContext::default());
    assert!(report.has("cand-1", FindingKind::WeakOracleIndependence));
    assert!(
        !report.blocks_review(),
        "deciding a weak oracle is exactly the human's job"
    );
    assert!(
        report.confidence["cand-1"].oracle_independence_is_weak(),
        "confidence travels with the report so QA sees it immediately"
    );
}
