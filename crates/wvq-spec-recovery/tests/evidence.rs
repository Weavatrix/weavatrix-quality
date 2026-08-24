//! Task 24: implementation evidence proposes intent, it never establishes it.

use wvq_spec_recovery::{
    ChangeNarrative, CodeDeltaSummary, ConfidenceLevel, EvidenceSource, EvidenceTier,
    IntentEvidence, NarrativeInput, TestsDelta, assess, establishes_intent, narrate,
    strongest_tier,
};

fn commit_title() -> IntentEvidence {
    IntentEvidence::new(
        EvidenceSource::CommitTitle,
        "fix sankey visual limit",
        "commit 39df3b2",
    )
}

fn code_delta() -> IntentEvidence {
    IntentEvidence::new(
        EvidenceSource::CodeDelta,
        "buildSankeyData grouped overflow values",
        "src/sankey.ts:40-44",
    )
}

fn changed_test() -> IntentEvidence {
    IntentEvidence::new(
        EvidenceSource::ChangedTest,
        "expects an Others node above the limit",
        "tests/sankey-others.spec.ts",
    )
}

fn declared_criterion() -> IntentEvidence {
    IntentEvidence::new(
        EvidenceSource::AcceptanceCriterion,
        "Overflow values SHALL be represented by an Others node.",
        "openspec/changes/sankey-others/specs/sankey/spec.md:12",
    )
}

fn pr_body() -> IntentEvidence {
    IntentEvidence::new(
        EvidenceSource::PullRequestBody,
        "Add Others node for Sankey visual limit",
        "PR #418",
    )
}

#[test]
fn a_commit_title_alone_cannot_establish_intent() {
    let only_hints = vec![
        commit_title(),
        IntentEvidence::new(EvidenceSource::BranchName, "fix/sankey-limit", "branch"),
        IntentEvidence::new(EvidenceSource::SymbolName, "groupOverflow", "src/sankey.ts"),
    ];
    assert!(!establishes_intent(&only_hints));
    assert_eq!(strongest_tier(&only_hints), Some(EvidenceTier::WeakHint));

    let confidence = assess(&only_hints);
    assert_eq!(confidence.intent_evidence, ConfidenceLevel::Weak);
    assert!(confidence.oracle_independence_is_weak());
}

#[test]
fn no_pile_of_implementation_evidence_establishes_intent() {
    let observed = vec![
        commit_title(),
        code_delta(),
        changed_test(),
        IntentEvidence::new(
            EvidenceSource::ChangedEndpoint,
            "GET /api/sankey/others",
            "routes",
        ),
        IntentEvidence::new(
            EvidenceSource::BehaviorDelta,
            "Others node appears",
            "run-77",
        ),
    ];
    assert!(
        !establishes_intent(&observed),
        "observed behaviour is not declared intent, however much of it there is"
    );
    assert_eq!(
        strongest_tier(&observed),
        Some(EvidenceTier::Implementation)
    );
}

#[test]
fn a_declared_criterion_outranks_observed_code() {
    let mixed = vec![code_delta(), changed_test(), declared_criterion()];
    assert!(establishes_intent(&mixed));
    assert_eq!(strongest_tier(&mixed), Some(EvidenceTier::DeclaredIntent));

    let confidence = assess(&mixed);
    assert_eq!(confidence.intent_evidence, ConfidenceLevel::Strong);
    assert_eq!(confidence.oracle_independence, ConfidenceLevel::Strong);
    assert!(!confidence.oracle_independence_is_weak());

    assert!(EvidenceTier::DeclaredIntent > EvidenceTier::ReviewedCollaboration);
    assert!(EvidenceTier::ReviewedCollaboration > EvidenceTier::Implementation);
    assert!(EvidenceTier::Implementation > EvidenceTier::WeakHint);
}

#[test]
fn implementation_plus_its_own_test_gives_weak_oracle_independence() {
    let confidence = assess(&[commit_title(), code_delta(), changed_test()]);
    assert_eq!(confidence.implementation_evidence, ConfidenceLevel::Strong);
    assert_eq!(confidence.intent_evidence, ConfidenceLevel::Weak);
    assert_eq!(
        confidence.oracle_independence,
        ConfidenceLevel::Weak,
        "a test written against the changed implementation is not an independent oracle"
    );
    assert_eq!(confidence.behavioral_observation, ConfidenceLevel::None);
}

#[test]
fn a_reviewed_pr_body_is_medium_not_declared() {
    let confidence = assess(&[pr_body(), code_delta()]);
    assert_eq!(confidence.intent_evidence, ConfidenceLevel::Medium);
    assert_eq!(confidence.oracle_independence, ConfidenceLevel::Medium);
    assert!(!establishes_intent(&[pr_body()]));
}

#[test]
fn the_narrative_keeps_hints_out_of_declared_intent() {
    let narrative: ChangeNarrative = narrate(NarrativeInput {
        change_cluster: "sankey-others".into(),
        base_revision: "rev-base".into(),
        head_revision: "rev-head".into(),
        evidence: vec![
            commit_title(),
            code_delta(),
            changed_test(),
            pr_body(),
            declared_criterion(),
        ],
        code_delta: CodeDeltaSummary {
            components: vec!["Sankey".into()],
            endpoints_added: vec!["GET /api/sankey/others".into()],
            endpoints_removed: vec![],
            changed_symbols: vec!["renderNodes".into(), "buildSankeyData".into()],
            public_symbols: Vec::new(),
        },
        tests_delta: TestsDelta {
            added: vec!["sankey-others.spec.ts".into()],
            ..TestsDelta::default()
        },
        behavior_delta: vec!["Others node appears above the visual limit".into()],
    });

    assert!(narrative.has_declared_intent());
    assert_eq!(narrative.declared_intent.len(), 2, "tier A and B only");
    assert!(
        narrative
            .declared_intent
            .iter()
            .all(|item| item.tier() >= EvidenceTier::ReviewedCollaboration),
        "no observed code or naming hint may appear as declared intent"
    );
    assert_eq!(
        narrative.declared_intent[0].tier(),
        EvidenceTier::DeclaredIntent,
        "declared intent is listed first"
    );
    assert_eq!(narrative.commit_hints, vec!["fix sankey visual limit"]);
    assert_eq!(
        narrative.code_delta.changed_symbols,
        vec!["buildSankeyData", "renderNodes"],
        "narrative output is sorted and therefore deterministic"
    );
    assert_eq!(narrative.base_revision, "rev-base");
    assert_eq!(narrative.head_revision, "rev-head");
}
