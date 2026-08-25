use std::collections::BTreeSet;

use wvq_domain::Severity;
use wvq_ui::{
    RESPONSIVE_SENTINEL_WIDTHS, ResponsivePolicy, ResponsiveProbe, UiCheck, UiEvidence,
    UiFindingState, UiIntegrityDelta, UiIntegrityFinding, next_responsive_probe,
    responsive_failure_intervals, responsive_probe_plan,
};

fn finding(width: u32) -> UiIntegrityFinding {
    let snapshot: wvq_ui::LayoutSnapshot = serde_json::from_value(serde_json::json!({
        "schema_v": 2,
        "revision": "head",
        "program": "checkout",
        "step": 1,
        "route": "/",
        "state_digest": "ab".repeat(32),
        "viewport": {"width": width, "height": 720},
        "responsive_breakpoints": [768],
        "responsive_breakpoints_complete": true,
        "document": {
            "scroll_width": width,
            "client_width": width,
            "scroll_height": 720,
            "client_height": 720
        },
        "nodes": [],
        "hit_tests": [],
        "truncated": false
    }))
    .unwrap();
    UiIntegrityFinding {
        check: UiCheck::ViewportOverflow,
        severity: Severity::Error,
        state: snapshot.state_key(),
        route: "/".into(),
        viewport: format!("{width}x720"),
        subject: "testid:toolbar".into(),
        counterpart: None,
        component_hint: None,
        nodes: vec!["n1".into()],
        evidence: UiEvidence {
            overflow_px: 12,
            ..UiEvidence::default()
        },
        detail: "overflows by 12px".into(),
    }
}

#[test]
fn bisects_only_an_observed_transition() {
    let policy = ResponsivePolicy {
        min_width: 320,
        max_width: 1_440,
        max_probes: 8,
        ..ResponsivePolicy::default()
    };
    assert_eq!(
        next_responsive_probe(&policy, &[probe(320, true), probe(1_440, false)]),
        Some(880)
    );
    assert_eq!(
        next_responsive_probe(&policy, &[probe(320, false), probe(1_440, false)]),
        None,
        "equal measured states do not expand into a fixed viewport matrix"
    );
}

fn probe(width: u32, failing: bool) -> ResponsiveProbe {
    ResponsiveProbe {
        width,
        delta: UiIntegrityDelta {
            new: failing.then(|| finding(width)).into_iter().collect(),
            ..UiIntegrityDelta::default()
        },
    }
}

#[test]
fn seeds_sentinels_and_css_neighbours_not_a_viewport_matrix() {
    let policy = ResponsivePolicy {
        min_width: 320,
        max_width: 1_440,
        max_probes: 32,
        ..ResponsivePolicy::default()
    };
    let plan = responsive_probe_plan(&policy, &BTreeSet::from([768, 1_024]));
    assert_eq!(
        plan.widths,
        vec![
            320, 360, 390, 414, 480, 640, 767, 768, 769, 1_023, 1_024, 1_025, 1_280, 1_440
        ]
    );
    assert!(!plan.truncated);
    assert_eq!(RESPONSIVE_SENTINEL_WIDTHS, [360, 390, 414, 480, 640, 768, 1_024, 1_280]);
}

#[test]
fn a_css_less_layout_still_measures_phone_and_tablet_sentinels() {
    let policy = ResponsivePolicy::default();
    let plan = responsive_probe_plan(&policy, &BTreeSet::new());
    assert_eq!(
        plan.widths,
        vec![320, 360, 390, 414, 480, 640, 768, 1_024, 1_280, 1_440]
    );
    assert!(!plan.widths.contains(&391), "sentinels are points, not a matrix");
    assert!(!plan.truncated);
}

#[test]
fn sentinels_outside_the_configured_range_are_omitted() {
    let policy = ResponsivePolicy {
        min_width: 800,
        max_width: 1_100,
        ..ResponsivePolicy::default()
    };
    let plan = responsive_probe_plan(&policy, &BTreeSet::from([768]));
    assert_eq!(plan.widths, vec![800, 1_024, 1_100]);
    assert!(!plan.truncated);
}

#[test]
fn a_tight_budget_keeps_range_bounds_before_extra_css_neighbours() {
    let policy = ResponsivePolicy {
        min_width: 320,
        max_width: 1_440,
        max_probes: 6,
        ..ResponsivePolicy::default()
    };
    let plan = responsive_probe_plan(&policy, &BTreeSet::from([768]));
    assert_eq!(plan.widths, vec![320, 360, 390, 414, 480, 1_440]);
    assert!(plan.widths.contains(&policy.max_width));
    assert!(plan.truncated);
}

#[test]
fn disabled_search_produces_no_probes() {
    let policy = ResponsivePolicy {
        enabled: false,
        ..ResponsivePolicy::default()
    };
    let plan = responsive_probe_plan(&policy, &BTreeSet::from([768]));
    assert!(plan.widths.is_empty());
    assert!(!plan.truncated);
}

#[test]
fn reports_the_exact_last_failing_width_after_bisection() {
    let policy = ResponsivePolicy {
        min_width: 320,
        max_width: 1_440,
        ..ResponsivePolicy::default()
    };
    let probes = vec![
        probe(320, true),
        probe(767, true),
        probe(768, false),
        probe(1_440, false),
    ];
    let intervals = responsive_failure_intervals(&policy, &probes);
    assert_eq!(intervals.len(), 1);
    assert_eq!(intervals[0].state, UiFindingState::New);
    assert_eq!(intervals[0].first_width, 320);
    assert_eq!(intervals[0].last_width, 767);
    assert!(intervals[0].lower_boundary_exact);
    assert!(intervals[0].upper_boundary_exact);
}
