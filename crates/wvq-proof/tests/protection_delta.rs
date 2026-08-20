//! Task 32: compare the safety nets. A local loss beats a global gain.

use wvq_domain::RevisionId;
use wvq_proof::{
    DeltaContext, FlowProtection, ProtectionDelta, ProtectionDeltaState, ProtectionSnapshot,
    protection_delta, snapshot, summarise,
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

fn snap(revision: &str, flows: Vec<FlowProtection>) -> ProtectionSnapshot {
    snapshot(&RevisionId::new(revision).unwrap(), flows).unwrap()
}

fn only(deltas: &[ProtectionDelta], name: &str) -> ProtectionDelta {
    deltas
        .iter()
        .find(|item| item.flow == name)
        .cloned()
        .unwrap_or_else(|| panic!("no delta for {name}: {deltas:?}"))
}

#[test]
fn equivalent_evidence_is_preserved() {
    let base = snap(
        "rev-base",
        vec![flow("f", "rev-base", &["t"], &["b1"], &["o"])],
    );
    let head = snap(
        "rev-head",
        vec![flow("f", "rev-head", &["t"], &["b1"], &["o"])],
    );
    let delta = only(
        &protection_delta(&base, &head, &DeltaContext::default()),
        "f",
    );
    assert_eq!(delta.state, ProtectionDeltaState::Preserved);
    assert!(!delta.state.is_regression());
}

#[test]
fn keeping_the_old_net_and_adding_more_is_improved() {
    let base = snap(
        "rev-base",
        vec![flow("f", "rev-base", &["t"], &["b1"], &["o"])],
    );
    let head = snap(
        "rev-head",
        vec![flow("f", "rev-head", &["t"], &["b1", "b2"], &["o"])],
    );
    let delta = only(
        &protection_delta(&base, &head, &DeltaContext::default()),
        "f",
    );
    assert_eq!(delta.state, ProtectionDeltaState::Improved);
}

#[test]
fn exercising_fewer_branches_is_degraded() {
    let base = snap(
        "rev-base",
        vec![flow("f", "rev-base", &["t"], &["b1", "b2"], &["o"])],
    );
    let head = snap(
        "rev-head",
        vec![flow("f", "rev-head", &["t"], &["b1"], &["o"])],
    );
    let delta = only(
        &protection_delta(&base, &head, &DeltaContext::default()),
        "f",
    );
    assert_eq!(delta.state, ProtectionDeltaState::Degraded);
    assert!(delta.state.is_regression());
}

#[test]
fn a_flow_that_loses_every_test_is_lost() {
    let base = snap(
        "rev-base",
        vec![flow("f", "rev-base", &["t"], &["b1"], &["o"])],
    );
    let head = snap("rev-head", vec![flow("f", "rev-head", &[], &[], &[])]);
    let delta = only(
        &protection_delta(&base, &head, &DeltaContext::default()),
        "f",
    );
    assert_eq!(delta.state, ProtectionDeltaState::Lost);
    assert_eq!(delta.lost_obligations, vec!["o"]);
}

#[test]
fn a_different_test_proving_the_same_thing_is_replaced() {
    let base = snap(
        "rev-base",
        vec![flow("f", "rev-base", &["old"], &["b1"], &["o"])],
    );
    let head = snap(
        "rev-head",
        vec![flow("f", "rev-head", &["new"], &["b1"], &["o"])],
    );
    let delta = only(
        &protection_delta(&base, &head, &DeltaContext::default()),
        "f",
    );
    assert_eq!(delta.state, ProtectionDeltaState::Replaced);
    assert!(
        !delta.state.is_regression(),
        "this can be perfectly healthy"
    );
}

#[test]
fn protection_that_follows_a_refactor_is_relocated() {
    let base = snap(
        "rev-base",
        vec![flow("old-flow", "rev-base", &["t"], &["b1"], &["o"])],
    );
    let head = snap(
        "rev-head",
        vec![flow("new-flow", "rev-head", &["t"], &["b1"], &["o"])],
    );
    let context = DeltaContext {
        relocations: vec![("old-flow".into(), "new-flow".into())],
        ..DeltaContext::default()
    };
    let deltas = protection_delta(&base, &head, &context);
    assert_eq!(deltas.len(), 1, "a move is not a removal plus an addition");
    assert_eq!(deltas[0].state, ProtectionDeltaState::Relocated);
}

#[test]
fn new_behaviour_without_a_test_is_new_unprotected() {
    let base = snap("rev-base", vec![]);
    let head = snap("rev-head", vec![flow("f", "rev-head", &[], &[], &[])]);
    let delta = only(
        &protection_delta(&base, &head, &DeltaContext::default()),
        "f",
    );
    assert_eq!(delta.state, ProtectionDeltaState::NewUnprotected);
    assert!(delta.state.is_regression());
}

#[test]
fn an_approved_removal_is_obsolete_not_lost() {
    let base = snap(
        "rev-base",
        vec![flow("old-endpoint", "rev-base", &["t"], &["b1"], &["o"])],
    );
    let head = snap("rev-head", vec![]);
    let context = DeltaContext {
        intentionally_removed: vec!["old-endpoint".into()],
        ..DeltaContext::default()
    };
    let delta = only(&protection_delta(&base, &head, &context), "old-endpoint");
    assert_eq!(delta.state, ProtectionDeltaState::ObsoleteRemoved);
    assert!(!delta.state.is_regression());

    // Without the approved removal the very same change is a loss.
    let delta = only(
        &protection_delta(&base, &head, &DeltaContext::default()),
        "old-endpoint",
    );
    assert_eq!(delta.state, ProtectionDeltaState::Lost);
}

#[test]
fn a_global_coverage_gain_never_hides_a_critical_local_loss() {
    // Head measurably covers strictly more overall, but the viewer-denied guard
    // stopped being executed. Spec §77: the critical finding wins.
    let base = snap(
        "rev-base",
        vec![flow(
            "viewer-auth",
            "rev-base",
            &["auth-viewer.spec"],
            &["viewer-denied"],
            &["viewer-cannot-delete"],
        )],
    );
    let head = snap(
        "rev-head",
        vec![flow(
            "viewer-auth",
            "rev-head",
            &["auth-viewer.spec", "extra.spec", "more.spec"],
            &["happy-path", "another-path", "third-path"],
            &["viewer-cannot-delete"],
        )],
    );
    let context = DeltaContext {
        critical_branches: vec!["viewer-denied".into()],
        ..DeltaContext::default()
    };

    let delta = only(&protection_delta(&base, &head, &context), "viewer-auth");
    assert_eq!(
        delta.state,
        ProtectionDeltaState::Lost,
        "more tests and more branches must not outvote a lost critical guard"
    );
    assert!(delta.lost_critical_protection());
    assert_eq!(delta.lost_critical_branches, vec!["viewer-denied"]);
    assert!(
        delta
            .reasons
            .iter()
            .any(|item| item.contains("does not offset")),
        "the report must say why the gain was not enough: {:?}",
        delta.reasons
    );
}

#[test]
fn the_summary_counts_every_state() {
    let base = snap(
        "rev-base",
        vec![
            flow("kept", "rev-base", &["t"], &["b"], &["o"]),
            flow("gone", "rev-base", &["t"], &["b"], &["o"]),
        ],
    );
    let head = snap(
        "rev-head",
        vec![
            flow("kept", "rev-head", &["t"], &["b"], &["o"]),
            flow("fresh", "rev-head", &[], &[], &[]),
        ],
    );
    let summary = summarise(&protection_delta(&base, &head, &DeltaContext::default()));
    assert_eq!(summary.preserved, 1);
    assert_eq!(summary.lost, 1);
    assert_eq!(summary.new_unprotected, 1);
}
