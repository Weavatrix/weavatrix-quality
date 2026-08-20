//! Task 31: protection is recorded per revision, and stale evidence is refused.

use wvq_domain::RevisionId;
use wvq_proof::{
    FlowProtection, HistoricalProof, ProtectionError, ReusePolicy, may_reuse, snapshot,
};

fn base() -> RevisionId {
    RevisionId::new("rev-base").unwrap()
}

fn protection(flow: &str, revision: &str, tests: &[&str]) -> FlowProtection {
    FlowProtection {
        flow: flow.into(),
        revision: revision.into(),
        tests: tests.iter().map(|item| (*item).to_string()).collect(),
        sessions: Vec::new(),
        covered_nodes: vec!["guard".into()],
        covered_branches: vec!["viewer-denied".into()],
        proven_obligations: vec!["viewer-cannot-delete".into()],
        proofs: vec!["P-811".into()],
    }
}

#[test]
fn a_base_flow_stores_its_tests_coverage_and_proof() {
    let snapshot = snapshot(
        &base(),
        vec![
            protection("viewer-deny", "rev-base", &["auth-viewer.spec"]),
            protection("sankey-render", "rev-base", &["sankey.spec", "legend.spec"]),
        ],
    )
    .unwrap();

    assert_eq!(snapshot.revision, "rev-base");
    assert_eq!(
        snapshot
            .flows
            .iter()
            .map(|item| item.flow.as_str())
            .collect::<Vec<_>>(),
        vec!["sankey-render", "viewer-deny"],
        "snapshots are sorted, so comparisons are deterministic"
    );

    let flow = snapshot.flow("viewer-deny").expect("flow is recorded");
    assert_eq!(flow.tests, vec!["auth-viewer.spec"]);
    assert_eq!(flow.proofs, vec!["P-811"]);
    assert_eq!(flow.proven_obligations, vec!["viewer-cannot-delete"]);
    assert!(flow.covers_branch("viewer-denied"));
    assert!(flow.is_protected());
}

#[test]
fn an_unprotected_flow_is_named_not_hidden() {
    let mut bare = protection("orphan-flow", "rev-base", &[]);
    bare.proofs.clear();
    let snapshot = snapshot(&base(), vec![bare]).unwrap();
    assert_eq!(snapshot.unprotected(), vec!["orphan-flow"]);
}

#[test]
fn evidence_without_a_revision_is_refused() {
    let error = snapshot(&base(), vec![protection("viewer-deny", "", &["spec"])]).unwrap_err();
    assert_eq!(
        error,
        ProtectionError::NotRevisionBound {
            flow: "viewer-deny".into()
        }
    );
}

#[test]
fn evidence_from_another_revision_is_refused() {
    let error = snapshot(
        &base(),
        vec![protection("viewer-deny", "rev-head", &["spec"])],
    )
    .unwrap_err();
    assert_eq!(
        error,
        ProtectionError::RevisionMismatch {
            expected: "rev-base".into(),
            found: "rev-head".into(),
        },
        "a head measurement must never be filed as base protection"
    );
}

#[test]
fn a_recent_proof_under_the_same_environment_may_be_reused() {
    let policy = ReusePolicy {
        max_age_revisions: 5,
        environment: "node20-chromium".into(),
    };
    let proof = HistoricalProof {
        id: "P-811".into(),
        age_revisions: 2,
        environment: "node20-chromium".into(),
        program: "auth-viewer.spec".into(),
    };
    assert!(may_reuse(&proof, &policy).is_ok());
}

#[test]
fn a_stale_or_drifted_proof_must_be_re_run() {
    let policy = ReusePolicy {
        max_age_revisions: 5,
        environment: "node20-chromium".into(),
    };

    let old = HistoricalProof {
        id: "P-700".into(),
        age_revisions: 42,
        environment: "node20-chromium".into(),
        program: "auth-viewer.spec".into(),
    };
    assert_eq!(
        may_reuse(&old, &policy).unwrap_err(),
        ProtectionError::TooOld {
            age: 42,
            allowed: 5
        }
    );

    let drifted = HistoricalProof {
        id: "P-810".into(),
        age_revisions: 1,
        environment: "node18-firefox".into(),
        program: "auth-viewer.spec".into(),
    };
    assert_eq!(
        may_reuse(&drifted, &policy).unwrap_err(),
        ProtectionError::EnvironmentDrift {
            expected: "node20-chromium".into(),
            found: "node18-firefox".into(),
        },
        "an incompatible environment makes the stored proof untrustworthy"
    );
}
