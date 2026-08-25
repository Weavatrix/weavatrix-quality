//! Axe/Storybook JSON becomes ordinary ratchet findings. HTML never lands.

use serde_json::json;
use wvq_domain::Severity;
use wvq_ui::{
    DocumentMetrics, LAYOUT_SNAPSHOT_SCHEMA_V, LayoutSnapshot, Rect, UiCheck, UiIntegrityPolicy,
    UiIntegritySnapshot, UiNode, UiNodeId, Viewport, import_a11y_violations, ratchet,
};

fn snapshot() -> LayoutSnapshot {
    LayoutSnapshot {
        schema_v: LAYOUT_SNAPSHOT_SCHEMA_V,
        revision: wvq_domain::RevisionId::new("rev-head").unwrap(),
        program: "checkout".into(),
        step: 3,
        route: "/checkout".into(),
        state_digest: wvq_domain::ContentHash::new("ab".repeat(32)).unwrap(),
        viewport: Viewport {
            width: 1280,
            height: 720,
        },
        responsive_breakpoints: Vec::new(),
        responsive_breakpoints_complete: true,
        document: DocumentMetrics::default(),
        nodes: vec![UiNode {
            id: UiNodeId::new("n1").unwrap(),
            test_id: Some("pay".into()),
            rects: vec![Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            }],
            visible: true,
            ..UiNode::default()
        }],
        hit_tests: Vec::new(),
        truncated: false,
    }
}

#[test]
fn axe_html_is_dropped_and_impact_sets_objective_severity() {
    let report = json!({
        "producer": "axe-core",
        "violations": [{
            "id": "button-name",
            "impact": "critical",
            "help": "Buttons must have discernible text",
            "nodes": [{
                "html": "<button onclick=\"steal(password)\">x</button>",
                "failureSummary": "Fix any of the following: secret",
                "target": ["button.pay"]
            }]
        }]
    });
    let (findings, truncated) = import_a11y_violations(&snapshot(), &report).unwrap();
    assert!(!truncated);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].check, UiCheck::ImportedA11y);
    assert_eq!(findings[0].check.id(), "WVQ-A11Y-IMPORT-001");
    assert_eq!(findings[0].severity, Severity::Error);
    assert_eq!(findings[0].subject, "axe:button-name:button.pay");
    let encoded = serde_json::to_string(&findings[0]).unwrap();
    assert!(!encoded.contains("steal"));
    assert!(!encoded.contains("password"));
    assert!(!encoded.contains("onclick"));
}

#[test]
fn storybook_nested_results_are_accepted() {
    let report = json!({
        "results": {
            "violations": [{
                "id": "label",
                "impact": "moderate",
                "nodes": [{ "target": ["#email"] }]
            }]
        }
    });
    let (findings, _) = import_a11y_violations(&snapshot(), &report).unwrap();
    assert_eq!(findings[0].severity, Severity::Warn);
    assert_eq!(findings[0].subject, "axe:label:#email");
    assert!(findings[0].detail.contains("storybook-a11y"));
}

#[test]
fn a_new_imported_error_blocks_and_an_existing_one_does_not() {
    let layout = snapshot();
    let report = json!({
        "violations": [{
            "id": "button-name",
            "impact": "serious",
            "nodes": [{ "target": ["button"] }]
        }]
    });
    let (findings, _) = import_a11y_violations(&layout, &report).unwrap();
    let policy = UiIntegrityPolicy {
        enabled: true,
        ..UiIntegrityPolicy::default()
    };
    let empty = UiIntegritySnapshot {
        revision: "base".into(),
        measured_states: [layout.state_key()].into_iter().collect(),
        ..UiIntegritySnapshot::default()
    };
    let head = UiIntegritySnapshot {
        revision: "head".into(),
        measured_states: [layout.state_key()].into_iter().collect(),
        findings: findings.clone(),
        ..UiIntegritySnapshot::default()
    };
    let fresh = ratchet(&empty, &head, &Default::default(), &policy);
    assert!(fresh.blocks());
    assert_eq!(fresh.new.len(), 1);

    let base = UiIntegritySnapshot {
        revision: "base".into(),
        measured_states: [layout.state_key()].into_iter().collect(),
        findings,
        ..UiIntegritySnapshot::default()
    };
    let legacy = ratchet(&base, &head, &Default::default(), &policy);
    assert!(!legacy.blocks());
    assert_eq!(legacy.existing.len(), 1);
}
