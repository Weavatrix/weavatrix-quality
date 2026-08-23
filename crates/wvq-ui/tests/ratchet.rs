//! Base/head UI ratchet: old debt must not block, a new regression must.

use std::collections::BTreeSet;

use wvq_domain::Severity;
use wvq_ui::{
    LAYOUT_SNAPSHOT_SCHEMA_V, LayoutSnapshot, Rect, UiCheck, UiEvidence, UiIntegrityFinding,
    UiIntegrityPolicy, UiIntegritySnapshot, UiNode, UiNodeId, Viewport, ratchet,
};

fn state_key(step: u32) -> wvq_ui::UiStateKey {
    LayoutSnapshot {
        schema_v: LAYOUT_SNAPSHOT_SCHEMA_V,
        revision: wvq_domain::RevisionId::new("rev").unwrap(),
        program: "checkout".into(),
        step,
        route: "/checkout".into(),
        state_digest: wvq_domain::ContentHash::new("ab".repeat(32)).unwrap(),
        viewport: Viewport {
            width: 1280,
            height: 720,
        },
        document: wvq_ui::DocumentMetrics::default(),
        nodes: vec![UiNode {
            id: UiNodeId::new("n1").unwrap(),
            rects: vec![Rect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            }],
            visible: true,
            ..UiNode::default()
        }],
        hit_tests: Vec::new(),
        truncated: false,
    }
    .state_key()
}

fn finding(check: UiCheck, subject: &str, severity: Severity, step: u32) -> UiIntegrityFinding {
    UiIntegrityFinding {
        check,
        severity,
        state: state_key(step),
        route: "/checkout".into(),
        viewport: "1280x720".into(),
        subject: subject.into(),
        counterpart: None,
        component_hint: None,
        nodes: Vec::new(),
        evidence: UiEvidence::default(),
        detail: "measured".into(),
    }
}

fn snapshot(revision: &str, findings: Vec<UiIntegrityFinding>) -> UiIntegritySnapshot {
    UiIntegritySnapshot {
        revision: revision.into(),
        measured_states: [state_key(3)].into_iter().collect(),
        findings,
        truncated: false,
    }
}

fn enabled() -> UiIntegrityPolicy {
    UiIntegrityPolicy {
        enabled: true,
        ..UiIntegrityPolicy::default()
    }
}

#[test]
fn a_duplicate_only_on_head_is_new() {
    let base = snapshot("base", Vec::new());
    let head = snapshot(
        "head",
        vec![finding(
            UiCheck::DuplicateDomId,
            "#save",
            Severity::Error,
            3,
        )],
    );
    let delta = ratchet(&base, &head, &BTreeSet::new(), &enabled());
    assert_eq!(delta.new.len(), 1);
    assert!(delta.existing.is_empty());
    assert!(delta.blocks());
}

#[test]
fn the_same_duplicate_on_both_revisions_is_existing_debt() {
    let item = finding(UiCheck::DuplicateDomId, "#save", Severity::Error, 3);
    let delta = ratchet(
        &snapshot("base", vec![item.clone()]),
        &snapshot("head", vec![item]),
        &BTreeSet::new(),
        &enabled(),
    );
    assert!(delta.new.is_empty());
    assert_eq!(delta.existing.len(), 1);
    assert!(
        !delta.blocks(),
        "unchanged legacy UI debt must not block adoption"
    );
}

#[test]
fn a_duplicate_removed_on_head_is_credited_as_fixed() {
    let item = finding(UiCheck::DuplicateDomId, "#save", Severity::Error, 3);
    let delta = ratchet(
        &snapshot("base", vec![item]),
        &snapshot("head", Vec::new()),
        &BTreeSet::new(),
        &enabled(),
    );
    assert_eq!(delta.fixed.len(), 1);
    assert!(!delta.blocks());
    assert_eq!(delta.fixed_fingerprints().len(), 1);
}

#[test]
fn a_previously_fixed_duplicate_that_comes_back_is_returned() {
    let item = finding(UiCheck::DuplicateDomId, "#save", Severity::Error, 3);
    let history: BTreeSet<String> = [item.fingerprint()].into_iter().collect();
    let delta = ratchet(
        &snapshot("base", Vec::new()),
        &snapshot("head", vec![item]),
        &history,
        &enabled(),
    );
    assert!(delta.new.is_empty());
    assert_eq!(delta.returned.len(), 1);
    assert!(delta.blocks(), "reintroducing a fixed defect blocks");
}

#[test]
fn an_explicit_exception_removes_a_finding_from_the_gate() {
    let item = finding(UiCheck::DuplicateDomId, "#save", Severity::Error, 3);
    let yaml = format!(
        "enabled: true\nexceptions:\n  - fingerprint: {}\n    reason: legacy widget\n    \
         reviewer: qa@example.invalid\n",
        item.fingerprint()
    );
    let value: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
    let policy = wvq_ui::parse_policy(&value, "2026-08-23").unwrap();
    let delta = ratchet(
        &snapshot("base", Vec::new()),
        &snapshot("head", vec![item]),
        &BTreeSet::new(),
        &policy,
    );
    assert!(delta.new.is_empty());
    assert_eq!(delta.excepted.len(), 1);
    assert!(!delta.blocks());
}

#[test]
fn the_same_defect_at_two_viewports_is_two_findings() {
    let mut wide = finding(UiCheck::ViewportOverflow, "button:Buy", Severity::Error, 3);
    let mut narrow = wide.clone();
    narrow.viewport = "767x900".into();
    narrow.state = {
        let mut key = state_key(3).to_string();
        key = key.replace("1280x720", "767x900");
        serde_json::from_value(serde_json::Value::String(key)).unwrap()
    };
    wide.viewport = "1280x720".into();
    assert_ne!(
        wide.fingerprint(),
        narrow.fingerprint(),
        "fixing the wide viewport does not fix the narrow one"
    );
}

#[test]
fn a_state_only_one_revision_measured_is_reported_unmeasured() {
    let mut base = snapshot("base", Vec::new());
    base.measured_states = [state_key(3)].into_iter().collect();
    let mut head = snapshot(
        "head",
        vec![finding(
            UiCheck::DuplicateDomId,
            "#save",
            Severity::Error,
            7,
        )],
    );
    head.measured_states = [state_key(3), state_key(7)].into_iter().collect();
    let delta = ratchet(&base, &head, &BTreeSet::new(), &enabled());
    assert!(
        delta.new.is_empty(),
        "base never looked here, so novelty is unknown rather than new"
    );
    assert_eq!(delta.existing.len(), 1);
    assert!(
        delta
            .unmeasured_states
            .iter()
            .any(|state| state.contains("#7")),
        "{:?}",
        delta.unmeasured_states
    );
}

#[test]
fn truncation_on_either_revision_propagates() {
    let mut head = snapshot("head", Vec::new());
    head.truncated = true;
    let delta = ratchet(
        &snapshot("base", Vec::new()),
        &head,
        &BTreeSet::new(),
        &enabled(),
    );
    assert!(delta.truncated);
}

#[test]
fn a_new_warning_does_not_block() {
    let delta = ratchet(
        &snapshot("base", Vec::new()),
        &snapshot(
            "head",
            vec![finding(
                UiCheck::TextClipping,
                "cell:Total",
                Severity::Warn,
                3,
            )],
        ),
        &BTreeSet::new(),
        &enabled(),
    );
    assert_eq!(delta.new.len(), 1);
    assert!(!delta.blocks());
}

#[test]
fn the_ratchet_is_deterministic() {
    let base = snapshot(
        "base",
        vec![finding(UiCheck::TextClipping, "cell:B", Severity::Warn, 3)],
    );
    let head = snapshot(
        "head",
        vec![
            finding(UiCheck::DuplicateDomId, "#z", Severity::Error, 3),
            finding(UiCheck::DuplicateDomId, "#a", Severity::Error, 3),
        ],
    );
    let first = ratchet(&base, &head, &BTreeSet::new(), &enabled());
    let second = ratchet(&base, &head, &BTreeSet::new(), &enabled());
    assert_eq!(first, second);
    let subjects: Vec<&str> = first.new.iter().map(|item| item.subject.as_str()).collect();
    assert_eq!(subjects, vec!["#a", "#z"]);
}
