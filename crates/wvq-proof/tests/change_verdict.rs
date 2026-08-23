//! Composite change-level verdict: priority order, axis independence, and the
//! three rules that keep one good axis from hiding another bad one.

use wvq_domain::{CheckId, Severity};
use wvq_proof::{
    AxisState, ChangeVerdictState, DebtAxis, DebtItem, Limitation, ProofOutcome, ProofVerdict,
    ProtectionAxis, ProtectionDelta, ProtectionDeltaState, ProtectionFinding, ProtectionSummary,
    StabilityAxis, UiFindingRef, UiIntegrityAxis, VerdictInputs, compose, debt_rule_blocks,
};

fn proven(id: &str) -> ProofOutcome {
    ProofOutcome {
        obligation: id.into(),
        requirement: "sankey.visual-limit".into(),
        verdict: ProofVerdict::Proven,
        mandatory: true,
    }
}

fn outcome(id: &str, verdict: ProofVerdict, mandatory: bool) -> ProofOutcome {
    ProofOutcome {
        obligation: id.into(),
        requirement: "sankey.visual-limit".into(),
        verdict,
        mandatory,
    }
}

fn finding(check: &str, severity: Severity, subject: &str) -> ProtectionFinding {
    ProtectionFinding {
        check: CheckId::new(check).unwrap(),
        severity,
        subject: subject.into(),
        detail: "measured".into(),
    }
}

fn ui(check: &str, severity: Severity, subject: &str) -> UiFindingRef {
    UiFindingRef {
        check: check.into(),
        severity,
        subject: subject.into(),
        route: "/analytics".into(),
        viewport: "1280x720".into(),
        detail: "0/9 hit-test points received events".into(),
    }
}

/// Every axis clean and in scope.
fn healthy() -> VerdictInputs {
    VerdictInputs {
        proofs: vec![proven("others-visible")],
        protection: ProtectionAxis {
            state: AxisState::Clean,
            measured: true,
            summary: ProtectionSummary {
                preserved: 3,
                ..ProtectionSummary::default()
            },
            ..ProtectionAxis::default()
        },
        debt: DebtAxis {
            state: AxisState::Clean,
            comparison_present: true,
            existing: 4,
            ..DebtAxis::default()
        },
        stability: StabilityAxis {
            state: AxisState::Clean,
            measured: true,
            ..StabilityAxis::default()
        },
        ui_integrity: UiIntegrityAxis {
            state: AxisState::Clean,
            ..UiIntegrityAxis::default()
        },
        ..VerdictInputs::default()
    }
}

#[test]
fn a_healthy_change_passes_with_no_reasons() {
    let verdict = compose(&healthy());
    assert_eq!(verdict.state, ChangeVerdictState::Pass);
    assert_eq!(verdict.state.as_str(), "PASS");
    assert!(verdict.blocking_reasons.is_empty());
    assert!(verdict.limitations.is_empty());
    assert!(!verdict.blocking());
    assert_eq!(verdict.exit_code(), 0);
}

#[test]
fn a_sealed_contradiction_blocks_first() {
    let mut inputs = healthy();
    inputs.proofs = vec![outcome("others-visible", ProofVerdict::Contradicted, true)];
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::Blocked);
    assert_eq!(verdict.exit_code(), 2);
    assert!(verdict.blocking());
    assert_eq!(verdict.blocking_reasons[0].rank, 1);
    assert_eq!(verdict.blocking_reasons[0].code, "WVQ-VERDICT-001");
    assert_eq!(verdict.proof.state, AxisState::Blocking);
}

#[test]
fn a_proven_obligation_cannot_hide_a_lost_critical_branch() {
    let mut inputs = healthy();
    // Behaviour still works; nothing guards it any more.
    inputs.protection = ProtectionAxis {
        state: AxisState::Blocking,
        measured: true,
        summary: ProtectionSummary {
            lost: 1,
            ..ProtectionSummary::default()
        },
        lost_flows: vec!["symbol:checkout".into()],
        lost_critical_branches: vec!["symbol:checkout#refund".into()],
        blocking_findings: vec![finding(
            "WVQ-PROTECT-001",
            Severity::Error,
            "symbol:checkout",
        )],
        warning_findings: Vec::new(),
    };
    let verdict = compose(&inputs);
    assert_eq!(verdict.proof.state, AxisState::Clean, "the proof is fine");
    assert_eq!(verdict.state, ChangeVerdictState::Blocked);
    assert_eq!(verdict.blocking_reasons[0].rank, 2);
    assert_eq!(verdict.blocking_reasons[0].axis, "protection");
    assert!(
        verdict
            .blocking_reasons
            .iter()
            .any(|item| item.subject == "symbol:checkout#refund"),
        "the exact lost branch is named: {:?}",
        verdict.blocking_reasons
    );
}

#[test]
fn a_global_coverage_gain_does_not_offset_a_local_protection_loss() {
    let mut inputs = healthy();
    inputs.protection = ProtectionAxis {
        state: AxisState::Blocking,
        measured: true,
        // Nine flows improved, one critical branch went. The improvement is
        // counted and still does not decide the verdict.
        summary: ProtectionSummary {
            improved: 9,
            lost: 1,
            ..ProtectionSummary::default()
        },
        lost_flows: vec!["symbol:checkout".into()],
        lost_critical_branches: vec!["symbol:checkout#refund".into()],
        blocking_findings: vec![finding(
            "WVQ-PROTECT-006",
            Severity::Error,
            "symbol:checkout",
        )],
        warning_findings: Vec::new(),
    };
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::Blocked);
    assert_eq!(verdict.protection.summary.improved, 9);
    assert_eq!(
        verdict
            .blocking_reasons
            .iter()
            .filter(|item| item.rank == 2)
            .count(),
        1,
        "WVQ-PROTECT-006 duplicates the critical-branch reason and is folded"
    );
}

#[test]
fn new_blocking_debt_blocks_but_existing_debt_does_not() {
    let mut inputs = healthy();
    inputs.debt = DebtAxis {
        state: AxisState::Clean,
        comparison_present: true,
        existing: 42,
        fixed: 3,
        ..DebtAxis::default()
    };
    assert_eq!(compose(&inputs).state, ChangeVerdictState::Pass);

    inputs.debt.new = vec![DebtItem {
        id: "arch-1".into(),
        rule: "WVQ-ARCH-002".into(),
        blocking: true,
    }];
    inputs.debt.state = AxisState::Blocking;
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::Blocked);
    assert_eq!(verdict.blocking_reasons[0].rank, 3);
    assert_eq!(verdict.debt.existing, 42, "legacy debt is still reported");
}

#[test]
fn a_new_non_blocking_debt_finding_is_a_warning_not_a_gate() {
    let mut inputs = healthy();
    inputs.debt.new = vec![DebtItem {
        id: "clone-9".into(),
        rule: "WVQ-CLONE-003".into(),
        blocking: false,
    }];
    inputs.debt.state = AxisState::Warnings;
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::PassWithWarnings);
    assert_eq!(verdict.exit_code(), 0);
    assert!(!verdict.blocking());
    assert_eq!(verdict.blocking_reasons[0].rank, 9);
}

#[test]
fn returned_blocking_debt_blocks_at_rank_five() {
    let mut inputs = healthy();
    inputs.debt.returned = vec![DebtItem {
        id: "api-4".into(),
        rule: "WVQ-API-001".into(),
        blocking: true,
    }];
    inputs.debt.state = AxisState::Blocking;
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::Blocked);
    assert_eq!(verdict.blocking_reasons[0].rank, 5);
    assert_eq!(verdict.blocking_reasons[0].code, "WVQ-VERDICT-005");
}

#[test]
fn a_mandatory_unproven_obligation_is_missing_evidence_not_a_pass() {
    let mut inputs = healthy();
    inputs.proofs = vec![outcome("others-visible", ProofVerdict::Unproven, true)];
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::NotEnoughEvidence);
    assert_eq!(verdict.state.as_str(), "NOT_ENOUGH_EVIDENCE");
    assert_eq!(verdict.exit_code(), 1);
    assert!(!verdict.blocking(), "missing evidence is not a failure");
    assert_eq!(verdict.blocking_reasons[0].rank, 4);
    assert_eq!(
        verdict.proof.unproven_mandatory,
        vec!["others-visible".to_owned()]
    );
}

#[test]
fn a_low_risk_unproven_obligation_reports_the_gap_without_rank_four() {
    let mut inputs = healthy();
    inputs.proofs = vec![outcome("cosmetic", ProofVerdict::Unproven, false)];
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::NotEnoughEvidence);
    assert!(verdict.proof.unproven_mandatory.is_empty());
    assert_eq!(
        verdict.blocking_reasons[0].rank, 7,
        "the axis is unmeasured, but no mandatory obligation is at stake"
    );
    assert!(verdict.limitations.iter().any(|item| item.axis == "proof"));
}

#[test]
fn an_unmeasured_axis_never_becomes_clean() {
    let mut inputs = healthy();
    inputs.protection = ProtectionAxis {
        state: AxisState::Unmeasured,
        measured: false,
        ..ProtectionAxis::default()
    };
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::NotEnoughEvidence);
    assert_eq!(verdict.blocking_reasons[0].rank, 7);
    assert_eq!(verdict.blocking_reasons[0].axis, "protection");
    assert_eq!(
        verdict.limitations,
        vec![Limitation {
            axis: "protection".into(),
            detail: "base and head protection were not both measured".into(),
        }]
    );
}

#[test]
fn a_backend_only_change_leaves_ui_not_applicable() {
    let inputs = healthy();
    assert_eq!(inputs.ui_integrity.state, AxisState::Clean);
    let mut backend = healthy();
    backend.ui_integrity = UiIntegrityAxis::default();
    let verdict = compose(&backend);
    assert_eq!(verdict.ui_integrity.state, AxisState::NotApplicable);
    assert_eq!(verdict.state, ChangeVerdictState::Pass);
    assert!(verdict.limitations.is_empty(), "absent is not unmeasured");
}

#[test]
fn a_proven_test_cannot_hide_a_new_ui_occlusion() {
    let mut inputs = healthy();
    inputs.ui_integrity = UiIntegrityAxis {
        state: AxisState::Blocking,
        new: vec![ui("WVQ-UI-LAYOUT-001", Severity::Error, "button:Export")],
        ..UiIntegrityAxis::default()
    };
    let verdict = compose(&inputs);
    assert_eq!(verdict.proof.state, AxisState::Clean);
    assert_eq!(verdict.state, ChangeVerdictState::Blocked);
    assert_eq!(verdict.exit_code(), 2);
    let reason = &verdict.blocking_reasons[0];
    assert_eq!(reason.rank, 3);
    assert_eq!(reason.axis, "ui_integrity");
    assert_eq!(reason.subject, "button:Export");
    assert!(reason.detail.contains("WVQ-UI-LAYOUT-001"));
    assert!(reason.detail.contains("1280x720"));
}

#[test]
fn existing_ui_debt_only_is_at_most_a_warning() {
    let mut inputs = healthy();
    inputs.ui_integrity = UiIntegrityAxis {
        state: AxisState::Warnings,
        existing: 7,
        ..UiIntegrityAxis::default()
    };
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::Pass);
    assert_eq!(verdict.ui_integrity.existing, 7);
    assert!(!verdict.blocking());
}

#[test]
fn a_new_warn_level_ui_finding_is_a_warning() {
    let mut inputs = healthy();
    inputs.ui_integrity = UiIntegrityAxis {
        state: AxisState::Warnings,
        new: vec![ui("WVQ-UI-LAYOUT-003", Severity::Warn, "cell:Total")],
        ..UiIntegrityAxis::default()
    };
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::PassWithWarnings);
    assert_eq!(verdict.blocking_reasons[0].rank, 9);
}

#[test]
fn a_truncated_layout_snapshot_is_not_enough_evidence() {
    let mut inputs = healthy();
    inputs.ui_integrity = UiIntegrityAxis {
        state: AxisState::Unmeasured,
        truncated: true,
        ..UiIntegrityAxis::default()
    };
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::NotEnoughEvidence);
    assert!(
        verdict
            .limitations
            .iter()
            .any(|item| item.axis == "ui_integrity" && item.detail.contains("node bound"))
    );
}

#[test]
fn a_missing_ui_state_names_what_was_not_collected() {
    let mut inputs = healthy();
    inputs.ui_integrity = UiIntegrityAxis {
        state: AxisState::Unmeasured,
        unmeasured_states: vec!["/checkout@767x900".into()],
        ..UiIntegrityAxis::default()
    };
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::NotEnoughEvidence);
    assert!(
        verdict
            .limitations
            .iter()
            .any(|item| item.detail.contains("/checkout@767x900"))
    );
}

#[test]
fn an_unresolved_mandatory_flake_needs_a_human() {
    let mut inputs = healthy();
    inputs.stability = StabilityAxis {
        state: AxisState::Warnings,
        measured: true,
        flaky: 1,
        unresolved_mandatory_flakes: vec!["tests/checkout.rs::refund".into()],
        ..StabilityAxis::default()
    };
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::NeedsHuman);
    assert_eq!(verdict.state.as_str(), "NEEDS_HUMAN");
    assert_eq!(verdict.exit_code(), 1);
    assert!(!verdict.blocking());
    assert_eq!(verdict.blocking_reasons[0].rank, 6);
}

#[test]
fn an_ambiguous_specification_needs_a_human() {
    let mut inputs = healthy();
    inputs.proofs = vec![outcome("unclear", ProofVerdict::HumanRequired, true)];
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::NeedsHuman);
    assert_eq!(verdict.blocking_reasons[0].code, "WVQ-VERDICT-010");
    assert_eq!(verdict.proof.state, AxisState::Warnings);
}

#[test]
fn an_exhausted_ai_budget_only_matters_for_an_unresolved_decision() {
    let mut inputs = healthy();
    inputs.ai.budget_exhausted = true;
    assert_eq!(
        compose(&inputs).state,
        ChangeVerdictState::Pass,
        "an exhausted budget with nothing pending is not a verdict"
    );
    inputs.ai.unresolved_decisions = vec!["heal:sankey-others".into()];
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::NeedsHuman);
    assert_eq!(verdict.blocking_reasons[0].rank, 8);
}

#[test]
fn the_ordinary_green_path_spends_no_runtime_tokens() {
    let verdict = compose(&healthy());
    assert_eq!(verdict.ai.runtime_tokens, 0);
    assert!(!verdict.ai.budget_exhausted);
}

#[test]
fn the_highest_priority_reason_decides_but_every_reason_is_kept() {
    let mut inputs = healthy();
    inputs.proofs = vec![outcome("others-visible", ProofVerdict::Contradicted, true)];
    inputs.protection = ProtectionAxis {
        state: AxisState::Blocking,
        measured: true,
        lost_critical_branches: vec!["symbol:checkout#refund".into()],
        ..ProtectionAxis::default()
    };
    inputs.debt.new = vec![DebtItem {
        id: "arch-1".into(),
        rule: "WVQ-ARCH-002".into(),
        blocking: true,
    }];
    inputs.ui_integrity = UiIntegrityAxis {
        state: AxisState::Blocking,
        new: vec![ui("WVQ-UI-DUP-001", Severity::Error, "#save")],
        ..UiIntegrityAxis::default()
    };
    inputs.stability.unresolved_mandatory_flakes = vec!["tests/a.rs::b".into()];
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::Blocked);
    let ranks: Vec<u8> = verdict
        .blocking_reasons
        .iter()
        .map(|item| item.rank)
        .collect();
    assert_eq!(ranks, vec![1, 2, 3, 3, 6], "sorted, nothing dropped");
}

#[test]
fn composition_is_deterministic() {
    let mut inputs = healthy();
    inputs.debt.new = vec![
        DebtItem {
            id: "z-last".into(),
            rule: "WVQ-ARCH-002".into(),
            blocking: true,
        },
        DebtItem {
            id: "a-first".into(),
            rule: "WVQ-ARCH-002".into(),
            blocking: true,
        },
    ];
    let first = compose(&inputs);
    let second = compose(&inputs);
    assert_eq!(first, second);
    let subjects: Vec<&str> = first
        .blocking_reasons
        .iter()
        .map(|item| item.subject.as_str())
        .collect();
    assert_eq!(subjects, vec!["a-first", "z-last"]);
}

#[test]
fn blocking_debt_families_are_architecture_api_and_security() {
    for rule in [
        "WVQ-ARCH-001",
        "WVQ-API-006",
        "wvq_sec_002",
        "weavatrix.security.hardcoded-secret",
    ] {
        assert!(debt_rule_blocks(rule), "{rule} must block");
    }
    for rule in [
        "WVQ-CLONE-003",
        "WVQ-DEAD-001",
        "WVQ-SIZE-002",
        "WVQ-HIST-004",
        "WVQ-GRAPH-002",
        "WVQ-COV-001",
        "rapid-growth",
    ] {
        assert!(!debt_rule_blocks(rule), "{rule} must not block");
    }
}

/// The composed verdict quotes the protection delta rather than re-deriving it,
/// so `ProtectionDeltaState::Lost` reaching the axis is what makes it blocking.
#[test]
fn a_lost_delta_reaches_the_axis_with_its_own_reasons() {
    let delta = ProtectionDelta {
        flow: "symbol:checkout".into(),
        state: ProtectionDeltaState::Lost,
        base_tests: vec!["tests/checkout.rs".into()],
        head_tests: Vec::new(),
        lost_critical_branches: vec!["symbol:checkout#refund".into()],
        lost_obligations: vec!["refund-allowed".into()],
        reasons: vec!["base had measured protection, head has no proof path".into()],
    };
    assert!(delta.state.is_regression());
    assert!(delta.lost_critical_protection());
    let mut inputs = healthy();
    inputs.protection = ProtectionAxis {
        state: AxisState::Blocking,
        measured: true,
        summary: ProtectionSummary {
            lost: 1,
            ..ProtectionSummary::default()
        },
        lost_flows: vec![delta.flow.clone()],
        lost_critical_branches: delta.lost_critical_branches.clone(),
        blocking_findings: Vec::new(),
        warning_findings: Vec::new(),
    };
    let verdict = compose(&inputs);
    assert_eq!(verdict.state, ChangeVerdictState::Blocked);
    assert_eq!(verdict.protection.lost_flows, vec!["symbol:checkout"]);
}
