//! Task 2: `OpenSpec` change deltas keep operation, text, scenarios, and provenance.

use std::path::{Path, PathBuf};

use wvq_spec::{
    ClauseKind, OpenSpecChange, RequirementOp, SpecError, read_change,
};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("openspec")
        .join("repo")
}

fn unix(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[test]
fn added_and_modified_preserve_text_scenarios_and_provenance() {
    let change = read_change(&fixture_root(), "sankey-others").unwrap();
    assert_eq!(change.id.as_str(), "sankey-others");
    assert_eq!(change.capabilities.len(), 1);

    let cap = &change.capabilities[0];
    assert_eq!(cap.capability, "sankey");
    assert!(
        unix(&cap.source).ends_with("openspec/changes/sankey-others/specs/sankey/spec.md"),
        "{}",
        unix(&cap.source)
    );
    assert_eq!(cap.operations.len(), 2);

    let RequirementOp::Added(added) = &cap.operations[0] else {
        panic!("first op should be ADDED");
    };
    assert_eq!(added.id.as_str(), "sankey.visual-limit-others");
    assert_eq!(added.name, "Visual Limit Others");
    assert!(
        added
            .text
            .contains("Overflow values SHALL be represented by an Others node")
    );
    assert_eq!(added.location.line, 5);
    assert_eq!(added.scenarios.len(), 1);

    let scenario = &added.scenarios[0];
    assert_eq!(scenario.id.as_str(), "overflow-grouped");
    assert_eq!(scenario.name, "Overflow grouped");
    assert_eq!(scenario.location.line, 8);
    assert_eq!(scenario.clauses.len(), 4);
    assert_eq!(scenario.clauses[0].kind, ClauseKind::Given);
    assert_eq!(
        scenario.clauses[0].text,
        "a Sankey chart with cardinality above the visual limit"
    );
    assert_eq!(scenario.clauses[1].kind, ClauseKind::When);
    assert_eq!(scenario.clauses[2].kind, ClauseKind::Then);
    assert_eq!(scenario.clauses[3].kind, ClauseKind::And);
    assert_eq!(
        scenario.clauses[3].text,
        "overflow values are grouped into that node"
    );

    let RequirementOp::Modified(modified) = &cap.operations[1] else {
        panic!("second op should be MODIFIED");
    };
    assert_eq!(modified.id.as_str(), "sankey.visual-limit");
    assert_eq!(modified.name, "Visual Limit");
    assert!(modified.text.contains("SHALL group overflow values"));
    assert_eq!(modified.scenarios[0].id.as_str(), "exact-limit");
}

#[test]
fn removed_requirement_keeps_reason_text() {
    let change = read_change(&fixture_root(), "retire-remember-me").unwrap();
    let RequirementOp::Removed(removed) = &change.capabilities[0].operations[0] else {
        panic!("expected REMOVED");
    };
    assert_eq!(change.capabilities[0].capability, "auth");
    assert_eq!(removed.id.as_str(), "auth.remember-me");
    assert_eq!(removed.name, "Remember Me");
    assert!(removed.text.contains("Deprecated in favor of 2FA"));
    assert!(removed.scenarios.is_empty());
}

#[test]
fn renamed_requirement_uses_from_to() {
    let change = read_change(&fixture_root(), "rename-visual-limit").unwrap();
    let RequirementOp::Renamed { from, to, location } = &change.capabilities[0].operations[0]
    else {
        panic!("expected RENAMED");
    };
    assert_eq!(from, "Visual Limit");
    assert_eq!(to, "Series Visual Limit");
    assert_eq!(location.line, 5);
}

#[test]
fn orphan_scenario_is_rejected() {
    let err = read_change(&fixture_root(), "malformed-orphan-scenario").unwrap_err();
    match err {
        SpecError::InvalidSyntax { line, message, .. } => {
            assert_eq!(line, 5);
            assert!(message.contains("scenario is not nested under a requirement"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn missing_change_fails_closed() {
    let err = read_change(&fixture_root(), "does-not-exist").unwrap_err();
    assert!(matches!(err, SpecError::ChangeNotFound(_)));
}

#[test]
fn given_when_then_helpers() {
    let change = read_change(&fixture_root(), "sankey-others").unwrap();
    assert_eq!(
        given_when_then(&change),
        (
            vec!["a Sankey chart with cardinality above the visual limit"],
            vec!["the chart is rendered"],
            vec![
                "an Others node is visible",
                "overflow values are grouped into that node"
            ]
        )
    );
}

fn given_when_then(change: &OpenSpecChange) -> (Vec<&str>, Vec<&str>, Vec<&str>) {
    let RequirementOp::Added(added) = &change.capabilities[0].operations[0] else {
        panic!("expected ADDED");
    };
    let mut given = Vec::new();
    let mut when = Vec::new();
    let mut then = Vec::new();
    let mut last = ClauseKind::Given;
    for clause in &added.scenarios[0].clauses {
        let target = match clause.kind {
            ClauseKind::Given => {
                last = ClauseKind::Given;
                &mut given
            }
            ClauseKind::When => {
                last = ClauseKind::When;
                &mut when
            }
            ClauseKind::Then => {
                last = ClauseKind::Then;
                &mut then
            }
            ClauseKind::And => match last {
                ClauseKind::When => &mut when,
                ClauseKind::Then | ClauseKind::And => &mut then,
                ClauseKind::Given => &mut given,
            },
        };
        target.push(clause.text.as_str());
    }
    (given, when, then)
}
