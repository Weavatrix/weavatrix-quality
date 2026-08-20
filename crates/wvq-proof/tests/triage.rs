//! Task 21: flake fingerprints, deterministic triage, safe healing.

use std::collections::BTreeMap;

use wvq_domain::{ObligationId, OracleSealId, ProgramId};
use wvq_proof::{
    FailureEvidence, FailureSignal, FlakeClass, HealEdit, HealError, TimingBucket, apply_heal,
    fingerprint_id, recover_target, triage,
};
use wvq_runtime::{ProgramSource, Target, TestAction, TestProgram, WaitCondition, semantic_target};

fn evidence() -> FailureEvidence {
    FailureEvidence {
        program: "sankey-others-replay".into(),
        obligation: Some("others-visible".into()),
        executor: "playwright".into(),
        seed: Some(7),
        state_digest: Some("abc".into()),
        stack_digest: Some("stack".into()),
        console_digest: Some("console".into()),
        network_digest: Some("net".into()),
        timing_bucket: Some(TimingBucket::Fast),
        ..FailureEvidence::default()
    }
}

fn program() -> TestProgram {
    TestProgram {
        schema_v: 1,
        id: ProgramId::new("sankey-others-replay").unwrap(),
        source: ProgramSource::Authored,
        obligations: vec![ObligationId::new("others-visible").unwrap()],
        preconditions: Vec::new(),
        steps: vec![
            TestAction::Navigate {
                route: "/sankey".into(),
            },
            TestAction::Activate {
                target: semantic_target("button", "Others"),
            },
            TestAction::Assert {
                obligation: ObligationId::new("others-visible").unwrap(),
            },
        ],
        data: BTreeMap::new(),
        faults: BTreeMap::new(),
        api_operations: BTreeMap::new(),
        evidence_policy: wvq_runtime::EvidencePolicy::default(),
        deterministic_seed: Some(7),
    }
}

#[test]
fn fingerprint_clusters_repeats() {
    let first = fingerprint_id(&evidence()).unwrap();
    let mut again = evidence();
    again.signals.insert(FailureSignal::EnvironmentMismatch);
    let second = fingerprint_id(&again).unwrap();
    assert_eq!(first, second, "triage flags are not part of identity");
    let mut other = evidence();
    other.stack_digest = Some("other-stack".into());
    assert_ne!(first, fingerprint_id(&other).unwrap());
}

#[test]
fn known_fingerprint_has_no_decision_packet() {
    let result = triage(&evidence(), true);
    assert_eq!(result.class, FlakeClass::Known);
    assert!(result.packet.is_none());
}

#[test]
fn unknown_emits_zero_token_packet() {
    let result = triage(&evidence(), false);
    assert_eq!(result.class, FlakeClass::Unknown);
    let packet = result.packet.expect("unknown needs a packet");
    assert_eq!(packet.runtime_tokens, 0);
    assert!(!packet.failed_candidates.is_empty());
}

#[test]
fn classifies_ordering_timing_network_seed_and_test_order() {
    let mut ordering = evidence();
    ordering.passed_when_reordered = Some(true);
    assert_eq!(triage(&ordering, false).class, FlakeClass::Ordering);

    let mut timing = evidence();
    timing.timing_bucket = Some(TimingBucket::Timeout);
    assert_eq!(triage(&timing, false).class, FlakeClass::Timing);

    let mut network = evidence();
    network.network_retries = 2;
    assert_eq!(triage(&network, false).class, FlakeClass::Network);

    let mut seed = evidence();
    seed.signals.insert(FailureSignal::SeedSensitive);
    assert_eq!(triage(&seed, false).class, FlakeClass::Seed);

    let mut order = evidence();
    order.passed_when_isolated = Some(true);
    assert_eq!(triage(&order, false).class, FlakeClass::TestOrder);

    let mut drift = evidence();
    drift.signals.insert(FailureSignal::SelectorMissing);
    assert_eq!(triage(&drift, false).class, FlakeClass::SelectorDrift);

    let mut regression = evidence();
    regression
        .signals
        .insert(FailureSignal::SameStateAlwaysFails);
    assert_eq!(
        triage(&regression, false).class,
        FlakeClass::ProductRegression
    );
}

#[test]
fn heal_retarget_and_wait_keep_seal_and_assertions() {
    let seal = OracleSealId::new("oseal-abc").unwrap();
    let recovered = recover_target(
        &semantic_target("button", "Others"),
        &Target {
            role: Some("button".into()),
            accessible_name: Some("Others".into()),
            test_id: Some("others-btn".into()),
            ..Target::default()
        },
    )
    .unwrap();
    let healed = apply_heal(
        &program(),
        &seal,
        &seal,
        &[
            HealEdit::Retarget {
                step: 1,
                target: recovered,
            },
            HealEdit::InsertWait {
                after: 0,
                condition: WaitCondition::Url {
                    route: "/sankey".into(),
                },
            },
        ],
        1,
    )
    .unwrap();
    assert_eq!(healed.revision, 2);
    assert_eq!(healed.seal, seal);
    assert!(matches!(&healed.program.steps[1], TestAction::Wait { .. }));
    assert!(matches!(
        healed.program.steps.last(),
        Some(TestAction::Assert { .. })
    ));
}

#[test]
fn heal_rejects_seal_mismatch_xpath_and_renamed_control() {
    let seal = OracleSealId::new("oseal-abc").unwrap();
    let other = OracleSealId::new("oseal-other").unwrap();
    let err = apply_heal(&program(), &seal, &other, &[], 1).unwrap_err();
    assert!(matches!(err, HealError::SealMismatch));

    let xpath = recover_target(
        &semantic_target("button", "Others"),
        &Target {
            fallback_css: Some("//button".into()),
            ..Target::default()
        },
    )
    .unwrap_err();
    assert!(matches!(xpath, HealError::Invalid(message) if message.contains("XPath")));

    let renamed = recover_target(
        &semantic_target("button", "Others"),
        &Target {
            role: Some("button".into()),
            accessible_name: Some("Something else".into()),
            ..Target::default()
        },
    )
    .unwrap_err();
    assert!(matches!(renamed, HealError::ExpectedResultChanged));
}

#[test]
fn heal_cannot_retarget_an_assertion() {
    let seal = OracleSealId::new("oseal-abc").unwrap();
    let err = apply_heal(
        &program(),
        &seal,
        &seal,
        &[HealEdit::Retarget {
            step: 2,
            target: semantic_target("button", "Others"),
        }],
        1,
    )
    .unwrap_err();
    assert!(matches!(err, HealError::AssertionChanged));
}
