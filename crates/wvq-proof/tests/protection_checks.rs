//! Task 34: WVQ-PROTECT-001 … 012.

use wvq_domain::RevisionId;
use wvq_domain::Severity;
use wvq_proof::{
    DeltaContext, FlowProtection, ProtectionCheckInput, ProtectionFinding, ProtectionPolicy,
    ProtectionTrend, TestChange, blocks, gate_protection, protection_delta, snapshot,
};

fn flow(
    name: &str,
    revision: &str,
    tests: &[&str],
    branches: &[&str],
    obligations: &[&str],
) -> FlowProtection {
    FlowProtection {
        flow: name.into(),
        revision: revision.into(),
        tests: tests.iter().map(|item| (*item).to_string()).collect(),
        sessions: Vec::new(),
        covered_nodes: Vec::new(),
        covered_branches: branches.iter().map(|item| (*item).to_string()).collect(),
        proven_obligations: obligations.iter().map(|item| (*item).to_string()).collect(),
        proofs: vec!["P-1".into()],
    }
}

/// Build deltas for one flow from a base and head protection pair.
fn deltas(
    base_flows: Vec<FlowProtection>,
    head_flows: Vec<FlowProtection>,
    context: &DeltaContext,
) -> Vec<wvq_proof::ProtectionDelta> {
    let base = snapshot(&RevisionId::new("rev-base").unwrap(), base_flows).unwrap();
    let head = snapshot(&RevisionId::new("rev-head").unwrap(), head_flows).unwrap();
    protection_delta(&base, &head, context)
}

fn high_risk(flow: &str) -> ProtectionPolicy {
    ProtectionPolicy {
        high_risk_flows: vec![flow.into()],
        substitution_ratio: 3,
    }
}

fn checks(findings: &[ProtectionFinding]) -> Vec<&str> {
    findings.iter().map(|item| item.check.as_str()).collect()
}

fn has(findings: &[ProtectionFinding], check: &str) -> bool {
    findings.iter().any(|item| item.check.as_str() == check)
}

fn severity_of(findings: &[ProtectionFinding], check: &str) -> Severity {
    findings
        .iter()
        .find(|item| item.check.as_str() == check)
        .unwrap_or_else(|| panic!("no {check} in {:?}", checks(findings)))
        .severity
}

#[test]
fn protect_001_and_007_fire_when_a_protected_flow_is_lost() {
    let input = ProtectionCheckInput {
        deltas: deltas(
            vec![flow("viewer-auth", "rev-base", &["t"], &["b"], &["o"])],
            vec![flow("viewer-auth", "rev-head", &[], &[], &[])],
            &DeltaContext::default(),
        ),
        policy: high_risk("viewer-auth"),
        ..ProtectionCheckInput::default()
    };
    let findings = gate_protection(&input);
    assert!(has(&findings, "WVQ-PROTECT-001"));
    assert!(has(&findings, "WVQ-PROTECT-007"));
    assert_eq!(severity_of(&findings, "WVQ-PROTECT-001"), Severity::Error);
    assert!(blocks(&findings));
}

#[test]
fn protect_006_and_010_fire_when_coverage_moves_off_a_critical_branch() {
    let context = DeltaContext {
        critical_branches: vec!["viewer-denied".into()],
        ..DeltaContext::default()
    };
    let input = ProtectionCheckInput {
        deltas: deltas(
            vec![flow(
                "viewer-auth",
                "rev-base",
                &["auth.spec"],
                &["viewer-denied"],
                &["o"],
            )],
            vec![flow(
                "viewer-auth",
                "rev-head",
                &["auth.spec", "extra.spec", "more.spec"],
                &["happy", "another", "third"],
                &["o"],
            )],
            &context,
        ),
        policy: ProtectionPolicy::default(),
        ..ProtectionCheckInput::default()
    };
    let findings = gate_protection(&input);
    assert!(has(&findings, "WVQ-PROTECT-006"));
    assert_eq!(
        severity_of(&findings, "WVQ-PROTECT-006"),
        Severity::Error,
        "a lost critical branch is an error even on a low-risk flow"
    );
    assert!(
        has(&findings, "WVQ-PROTECT-010"),
        "more tests plus a dropped critical branch is suspicious substitution"
    );
    assert!(blocks(&findings));
}

#[test]
fn protect_004_records_a_healthy_replacement_without_warning() {
    let input = ProtectionCheckInput {
        deltas: deltas(
            vec![flow("f", "rev-base", &["old.spec"], &["b"], &["o"])],
            vec![flow("f", "rev-head", &["new.spec"], &["b"], &["o"])],
            &DeltaContext::default(),
        ),
        ..ProtectionCheckInput::default()
    };
    let findings = gate_protection(&input);
    assert!(has(&findings, "WVQ-PROTECT-004"));
    assert_eq!(severity_of(&findings, "WVQ-PROTECT-004"), Severity::Info);
    assert!(!blocks(&findings));
}

#[test]
fn protect_008_accepts_an_approved_removal() {
    let context = DeltaContext {
        intentionally_removed: vec!["old-endpoint".into()],
        ..DeltaContext::default()
    };
    let input = ProtectionCheckInput {
        deltas: deltas(
            vec![flow("old-endpoint", "rev-base", &["t"], &["b"], &["o"])],
            vec![],
            &context,
        ),
        policy: high_risk("old-endpoint"),
        ..ProtectionCheckInput::default()
    };
    let findings = gate_protection(&input);
    assert!(has(&findings, "WVQ-PROTECT-008"));
    assert!(!has(&findings, "WVQ-PROTECT-001"));
    assert!(!blocks(&findings), "an approved removal must not block");
}

#[test]
fn protect_009_flags_new_behaviour_with_no_proof() {
    let input = ProtectionCheckInput {
        deltas: deltas(
            vec![],
            vec![flow("new-flow", "rev-head", &[], &[], &[])],
            &DeltaContext::default(),
        ),
        policy: high_risk("new-flow"),
        ..ProtectionCheckInput::default()
    };
    let findings = gate_protection(&input);
    assert_eq!(severity_of(&findings, "WVQ-PROTECT-009"), Severity::Error);
}

#[test]
fn protect_002_flags_a_test_that_survives_but_stops_protecting() {
    let input = ProtectionCheckInput {
        tests: vec![TestChange {
            test: "auth.spec".into(),
            flow: "viewer-auth".into(),
            survives: true,
            lost_flows: vec!["viewer-auth".into()],
            ..TestChange::default()
        }],
        policy: high_risk("viewer-auth"),
        ..ProtectionCheckInput::default()
    };
    let findings = gate_protection(&input);
    assert_eq!(severity_of(&findings, "WVQ-PROTECT-002"), Severity::Error);
}

#[test]
fn protect_003_flags_a_deleted_sole_proof_path() {
    let input = ProtectionCheckInput {
        tests: vec![TestChange {
            test: "auth.spec".into(),
            flow: "viewer-auth".into(),
            survives: false,
            lost_obligations: vec!["viewer-cannot-delete".into()],
            replaced_by: None,
            ..TestChange::default()
        }],
        ..ProtectionCheckInput::default()
    };
    assert!(has(&gate_protection(&input), "WVQ-PROTECT-003"));

    let replaced = ProtectionCheckInput {
        tests: vec![TestChange {
            replaced_by: Some("new.spec".into()),
            ..TestChange {
                test: "auth.spec".into(),
                flow: "viewer-auth".into(),
                survives: false,
                lost_obligations: vec!["viewer-cannot-delete".into()],
                ..TestChange::default()
            }
        }],
        ..ProtectionCheckInput::default()
    };
    assert!(
        !has(&gate_protection(&replaced), "WVQ-PROTECT-003"),
        "an equivalent replacement is not a lost proof path"
    );
}

#[test]
fn protect_005_refuses_a_weakened_assertion_without_a_new_seal() {
    let weakened = TestChange {
        test: "auth.spec".into(),
        flow: "viewer-auth".into(),
        survives: true,
        assertions_weakened: true,
        ..TestChange::default()
    };
    let input = ProtectionCheckInput {
        tests: vec![weakened.clone()],
        ..ProtectionCheckInput::default()
    };
    assert_eq!(
        severity_of(&gate_protection(&input), "WVQ-PROTECT-005"),
        Severity::Error
    );

    let sealed = ProtectionCheckInput {
        tests: vec![TestChange {
            new_oracle_seal: true,
            ..weakened
        }],
        ..ProtectionCheckInput::default()
    };
    assert!(
        !has(&gate_protection(&sealed), "WVQ-PROTECT-005"),
        "a new OracleSeal authorises the change"
    );
}

#[test]
fn protect_011_flags_a_test_adapted_to_the_implementation() {
    let adapted = TestChange {
        test: "sankey.spec".into(),
        flow: "sankey".into(),
        survives: true,
        changed_with_implementation: true,
        ..TestChange::default()
    };
    let input = ProtectionCheckInput {
        tests: vec![adapted.clone()],
        ..ProtectionCheckInput::default()
    };
    let findings = gate_protection(&input);
    assert!(has(&findings, "WVQ-PROTECT-011"));
    assert!(findings.iter().any(|item| {
        item.detail
            .contains("POSSIBLE_TEST_ADAPTATION_TO_IMPLEMENTATION")
    }));

    let declared = ProtectionCheckInput {
        tests: vec![TestChange {
            declared_spec_delta: true,
            ..adapted
        }],
        ..ProtectionCheckInput::default()
    };
    assert!(
        !has(&gate_protection(&declared), "WVQ-PROTECT-011"),
        "a declared SpecDelta explains the change"
    );
}

#[test]
fn protect_012_spots_erosion_no_single_change_would_show() {
    let eroding = ProtectionCheckInput {
        trends: vec![ProtectionTrend {
            flow: "viewer-auth".into(),
            protectors: vec![3, 3, 2, 2, 1],
        }],
        ..ProtectionCheckInput::default()
    };
    let findings = gate_protection(&eroding);
    assert_eq!(severity_of(&findings, "WVQ-PROTECT-012"), Severity::Warn);
    assert!(findings[0].detail.contains("3 protector(s) to 1"));

    let steady = ProtectionCheckInput {
        trends: vec![ProtectionTrend {
            flow: "viewer-auth".into(),
            protectors: vec![2, 3, 2],
        }],
        ..ProtectionCheckInput::default()
    };
    assert!(
        !has(&gate_protection(&steady), "WVQ-PROTECT-012"),
        "a blip is not a trend"
    );
}

#[test]
fn a_preserved_flow_produces_nothing() {
    let input = ProtectionCheckInput {
        deltas: deltas(
            vec![flow("f", "rev-base", &["t"], &["b"], &["o"])],
            vec![flow("f", "rev-head", &["t"], &["b"], &["o"])],
            &DeltaContext::default(),
        ),
        ..ProtectionCheckInput::default()
    };
    assert!(gate_protection(&input).is_empty());
}
