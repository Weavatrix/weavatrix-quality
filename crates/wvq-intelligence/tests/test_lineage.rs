//! Task 30: a surviving test name is not surviving protection.

use wvq_intelligence::{TestFacts, TestLineage, TestLineageState, track_lineage};

fn test_at(revision: &str, id: &str, name: &str, flows: &[&str]) -> TestFacts {
    TestFacts {
        id: id.into(),
        revision: revision.into(),
        name: name.into(),
        path: format!("tests/{name}.spec.ts"),
        body_digest: format!("digest-{name}"),
        renamed_from: None,
        covered_nodes: Vec::new(),
        covered_obligations: Vec::new(),
        covered_flows: flows.iter().map(|item| (*item).to_string()).collect(),
    }
}

fn only(records: Vec<TestLineage>) -> TestLineage {
    assert_eq!(records.len(), 1, "expected one record: {records:?}");
    records.into_iter().next().expect("exactly one record")
}

#[test]
fn an_untouched_test_is_unchanged() {
    let base = vec![test_at("rev-base", "b1", "auth-viewer", &["viewer-deny"])];
    let head = vec![test_at("rev-head", "h1", "auth-viewer", &["viewer-deny"])];
    let record = only(track_lineage(&base, &head));
    assert_eq!(record.state, TestLineageState::Unchanged);
    assert_eq!(record.matched_on, "test_name");
    assert!(!record.protection_changed);
    assert!(!record.is_phantom());
}

#[test]
fn a_git_rename_keeps_lineage() {
    let base = vec![test_at("rev-base", "b1", "auth-viewer", &["viewer-deny"])];
    let head = vec![TestFacts {
        renamed_from: Some("b1".into()),
        ..test_at("rev-head", "h1", "permissions-viewer", &["viewer-deny"])
    }];
    let record = only(track_lineage(&base, &head));
    assert_eq!(record.state, TestLineageState::Renamed);
    assert_eq!(record.matched_on, "git_rename");
    assert!(
        !record.protection_changed,
        "a rename alone does not change what the test executes"
    );
}

#[test]
fn a_changed_body_under_the_same_name_is_modified() {
    let base = vec![test_at("rev-base", "b1", "auth-viewer", &["viewer-deny"])];
    let head = vec![TestFacts {
        body_digest: "digest-rewritten".into(),
        ..test_at("rev-head", "h1", "auth-viewer", &["viewer-deny"])
    }];
    assert_eq!(
        only(track_lineage(&base, &head)).state,
        TestLineageState::Modified
    );
}

#[test]
fn a_removed_test_reports_everything_it_protected() {
    let mut base_test = test_at("rev-base", "b1", "auth-viewer", &["viewer-deny"]);
    base_test.covered_obligations = vec!["viewer-cannot-delete".into()];
    let record = only(track_lineage(&[base_test], &[]));
    assert_eq!(record.state, TestLineageState::Removed);
    assert!(!record.state.survives());
    assert_eq!(record.lost_flows, vec!["viewer-deny"]);
    assert_eq!(record.lost_obligations, vec!["viewer-cannot-delete"]);
}

#[test]
fn a_surviving_test_that_stopped_reaching_the_flow_is_a_phantom() {
    // Identical name, identical source, but it no longer executes the guard.
    let base = vec![test_at(
        "rev-base",
        "b1",
        "auth-viewer",
        &["viewer-deny", "sankey-render"],
    )];
    let head = vec![test_at("rev-head", "h1", "auth-viewer", &["sankey-render"])];
    let record = only(track_lineage(&base, &head));

    assert_eq!(
        record.state,
        TestLineageState::Unchanged,
        "the source really is unchanged"
    );
    assert!(
        record.protection_changed,
        "but the protection it provides is not"
    );
    assert_eq!(record.lost_flows, vec!["viewer-deny"]);
    assert!(
        record.is_phantom(),
        "a green, present, no-longer-guarding test must be visible"
    );
}

#[test]
fn a_newly_added_test_is_reported_with_what_it_gained() {
    let head = vec![test_at(
        "rev-head",
        "h1",
        "sankey-others",
        &["others-detail"],
    )];
    let record = only(track_lineage(&[], &head));
    assert_eq!(record.state, TestLineageState::Added);
    assert_eq!(record.gained_flows, vec!["others-detail"]);
    assert!(record.lost_flows.is_empty());
}

#[test]
fn one_test_split_into_two_keeps_both_halves_linked() {
    let base = vec![test_at(
        "rev-base",
        "b1",
        "auth",
        &["viewer-deny", "admin-allow"],
    )];
    let head = vec![
        TestFacts {
            renamed_from: Some("b1".into()),
            ..test_at("rev-head", "h1", "auth-viewer", &["viewer-deny"])
        },
        TestFacts {
            renamed_from: Some("b1".into()),
            ..test_at("rev-head", "h2", "auth-admin", &["admin-allow"])
        },
    ];
    let records = track_lineage(&base, &head);
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .all(|item| item.state == TestLineageState::Split)
    );
    assert!(
        records
            .iter()
            .all(|item| item.base.as_deref() == Some("b1")),
        "both halves stay attached to the test they came from"
    );
}

#[test]
fn two_tests_folded_into_one_are_merged() {
    let base = vec![
        test_at("rev-base", "b1", "auth-viewer", &["viewer-deny"]),
        test_at("rev-base", "b2", "auth-admin", &["admin-allow"]),
    ];
    let head = vec![
        TestFacts {
            renamed_from: Some("b1".into()),
            ..test_at("rev-head", "h1", "auth", &["viewer-deny", "admin-allow"])
        },
        TestFacts {
            renamed_from: Some("b2".into()),
            ..test_at(
                "rev-head",
                "h1-dup",
                "auth",
                &["viewer-deny", "admin-allow"],
            )
        },
    ];
    let records = track_lineage(&base, &head);
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .all(|item| item.state == TestLineageState::Renamed),
        "distinct head tests are not a merge: {records:?}"
    );

    // Now genuinely fold both into a single head test.
    let single = vec![TestFacts {
        renamed_from: Some("b1".into()),
        covered_obligations: vec!["viewer-cannot-delete".into(), "admin-can-delete".into()],
        ..test_at("rev-head", "h1", "auth", &["viewer-deny", "admin-allow"])
    }];
    let mut base_with_obligations = base;
    base_with_obligations[0].covered_obligations = vec!["viewer-cannot-delete".into()];
    base_with_obligations[1].covered_obligations = vec!["admin-can-delete".into()];
    let records = track_lineage(&base_with_obligations, &single);
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .all(|item| item.state == TestLineageState::Merged),
        "both base tests now point at one head test: {records:?}"
    );
}

#[test]
fn a_test_matched_only_by_the_obligation_it_proves() {
    let mut base_test = test_at("rev-base", "b1", "old-name", &["viewer-deny"]);
    base_test.covered_obligations = vec!["viewer-cannot-delete".into()];
    let mut head_test = test_at("rev-head", "h1", "totally-different", &["viewer-deny"]);
    head_test.covered_obligations = vec!["viewer-cannot-delete".into()];

    let record = only(track_lineage(&[base_test], &[head_test]));
    assert_eq!(record.matched_on, "covered_obligation");
    assert_eq!(record.state, TestLineageState::Renamed);
    assert!(!record.protection_changed);
}
