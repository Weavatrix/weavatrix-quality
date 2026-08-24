//! Detector behaviour, and the false positives it must not produce.
//!
//! Most of this file is the second kind. Overlap is how interfaces are built —
//! icons inside inputs, badges on avatars, dialogs on backdrops, tooltips on
//! triggers, repeated row actions — and a detector that flags them is worse
//! than no detector at all.

use wvq_domain::Severity;
use wvq_ui::{
    DocumentMetrics, HitTestSample, LAYOUT_SNAPSHOT_SCHEMA_V, LayoutSnapshot, Point, Rect, UiCheck,
    UiIntegrityPolicy, UiNode, UiNodeId, Viewport, detect,
};

fn id(raw: &str) -> UiNodeId {
    UiNodeId::new(raw).unwrap()
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// A visible, enabled, clickable node.
fn control(node_id: &str, role: &str, name: &str, bounds: Rect) -> UiNode {
    UiNode {
        id: id(node_id),
        role: Some(role.into()),
        accessible_name: Some(name.into()),
        rects: vec![bounds],
        visible: true,
        interactive: true,
        enabled: true,
        pointer_events: true,
        ..UiNode::default()
    }
}

/// A visible, non-interactive node.
fn panel(node_id: &str, bounds: Rect) -> UiNode {
    UiNode {
        id: id(node_id),
        rects: vec![bounds],
        visible: true,
        pointer_events: true,
        ..UiNode::default()
    }
}

fn snapshot(nodes: Vec<UiNode>) -> LayoutSnapshot {
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
        document: DocumentMetrics {
            scroll_width: 1280.0,
            client_width: 1280.0,
            scroll_height: 720.0,
            client_height: 720.0,
        },
        nodes,
        hit_tests: Vec::new(),
        truncated: false,
    }
}

fn enabled_policy() -> UiIntegrityPolicy {
    UiIntegrityPolicy {
        enabled: true,
        ..UiIntegrityPolicy::default()
    }
}

/// Nine hit-test points on `target`, all intercepted by `blocker`.
fn fully_blocked(target: &str, blocker: &str) -> Vec<HitTestSample> {
    (0..9)
        .map(|index| HitTestSample {
            target: id(target),
            point: Point {
                x: f64::from(index),
                y: f64::from(index),
            },
            topmost: Some(id(blocker)),
            stack: vec![id(blocker), id(target)],
        })
        .collect()
}

/// Nine hit-test points on `target`, all reaching it.
fn fully_reachable(target: &str) -> Vec<HitTestSample> {
    (0..9)
        .map(|index| HitTestSample {
            target: id(target),
            point: Point {
                x: f64::from(index),
                y: f64::from(index),
            },
            topmost: Some(id(target)),
            stack: vec![id(target)],
        })
        .collect()
}

fn checks(snapshot: &LayoutSnapshot) -> Vec<(UiCheck, String, Severity)> {
    detect(snapshot, &enabled_policy())
        .unwrap()
        .findings
        .into_iter()
        .map(|finding| (finding.check, finding.subject, finding.severity))
        .collect()
}

#[test]
fn standards_derived_accessibility_rules_report_only_measured_failures() {
    let mut unnamed = control("unnamed", "button", "", rect(10.0, 10.0, 80.0, 30.0));
    unnamed.accessible_name = None;
    unnamed.tag = Some("button".into());
    unnamed.focusable = Some(true);

    let mut email = control(
        "email",
        "textbox",
        "Email address",
        rect(10.0, 50.0, 180.0, 30.0),
    );
    email.tag = Some("input".into());
    email.input_type = Some("email".into());
    email.focusable = Some(true);
    email.label_associated = Some(false);

    let mut sealed_button = control(
        "sealed",
        "button",
        "Checkout",
        rect(10.0, 90.0, 100.0, 30.0),
    );
    sealed_button.tag = Some("div".into());
    sealed_button.required_by_oracle = true;
    sealed_button.focusable = Some(false);

    let mut checkbox = control(
        "terms",
        "checkbox",
        "Accept terms",
        rect(10.0, 130.0, 120.0, 30.0),
    );
    checkbox.tag = Some("div".into());
    checkbox.focusable = Some(true);

    let mut dialog = panel("dialog", rect(250.0, 20.0, 300.0, 200.0));
    dialog.tag = Some("div".into());
    dialog.role = Some("dialog".into());
    dialog.accessible_name = None;
    dialog.modal = Some(true);
    dialog.contains_focus = Some(false);

    let found = checks(&snapshot(vec![
        unnamed,
        email,
        sealed_button,
        checkbox,
        dialog,
    ]));
    assert!(found.iter().any(|(check, subject, severity)| {
        *check == UiCheck::AccessibleName
            && subject == "button:<unnamed>"
            && *severity == Severity::Warn
    }));
    assert!(found.iter().any(|(check, subject, severity)| {
        *check == UiCheck::FormLabel
            && subject == "textbox:Email address"
            && *severity == Severity::Warn
    }));
    assert!(found.iter().any(|(check, subject, severity)| {
        *check == UiCheck::KeyboardReachability
            && subject == "button:Checkout"
            && *severity == Severity::Error
    }));
    assert!(found.iter().any(|(check, subject, severity)| {
        *check == UiCheck::RoleState
            && subject == "checkbox:Accept terms"
            && *severity == Severity::Warn
    }));
    assert!(found.iter().any(|(check, _, severity)| {
        *check == UiCheck::DialogName && *severity == Severity::Warn
    }));
    assert!(found.iter().any(|(check, _, severity)| {
        *check == UiCheck::DialogFocus && *severity == Severity::Warn
    }));
}

#[test]
fn accessible_native_controls_and_a_focused_named_dialog_stay_clean() {
    let mut email = control(
        "email",
        "textbox",
        "Email address",
        rect(10.0, 10.0, 180.0, 30.0),
    );
    email.tag = Some("input".into());
    email.input_type = Some("email".into());
    email.focusable = Some(true);
    email.label_associated = Some(true);
    email.native_disabled = Some(false);

    let mut checkbox = control(
        "terms",
        "checkbox",
        "Accept terms",
        rect(10.0, 50.0, 120.0, 30.0),
    );
    checkbox.tag = Some("input".into());
    checkbox.input_type = Some("checkbox".into());
    checkbox.focusable = Some(true);
    checkbox.label_associated = Some(true);
    checkbox.native_disabled = Some(false);

    let mut dialog = panel("dialog", rect(250.0, 20.0, 300.0, 200.0));
    dialog.tag = Some("dialog".into());
    dialog.role = Some("dialog".into());
    dialog.accessible_name = Some("Confirm order".into());
    dialog.modal = Some(true);
    dialog.contains_focus = Some(true);

    let found = checks(&snapshot(vec![email, checkbox, dialog]));
    assert!(
        found.iter().all(|(check, _, _)| !matches!(
            check,
            UiCheck::AccessibleName
                | UiCheck::FormLabel
                | UiCheck::KeyboardReachability
                | UiCheck::RoleState
                | UiCheck::DialogName
                | UiCheck::DialogFocus
        )),
        "accessible controls must stay clean: {found:?}"
    );
}

// ---------------------------------------------------------------------------
// Duplicate identity
// ---------------------------------------------------------------------------

#[test]
fn two_visible_nodes_sharing_a_dom_id_is_an_error() {
    let mut first = control("n1", "button", "Save", rect(0.0, 0.0, 80.0, 30.0));
    first.dom_id = Some("save".into());
    let mut second = control("n2", "button", "Save", rect(200.0, 0.0, 80.0, 30.0));
    second.dom_id = Some("save".into());
    let found = checks(&snapshot(vec![first, second]));
    assert!(
        found.contains(&(UiCheck::DuplicateDomId, "#save".into(), Severity::Error)),
        "{found:?}"
    );
}

#[test]
fn a_hidden_clone_does_not_duplicate_a_dom_id() {
    let mut first = control("n1", "button", "Save", rect(0.0, 0.0, 80.0, 30.0));
    first.dom_id = Some("save".into());
    let mut hidden = control("n2", "button", "Save", rect(0.0, 0.0, 80.0, 30.0));
    hidden.dom_id = Some("save".into());
    hidden.visible = false;
    let found = checks(&snapshot(vec![first, hidden]));
    assert!(
        !found
            .iter()
            .any(|(check, ..)| *check == UiCheck::DuplicateDomId),
        "an unrendered template clone is not a runtime ambiguity: {found:?}"
    );
}

#[test]
fn two_visible_nodes_sharing_a_test_id_is_an_error() {
    let mut first = control("n1", "button", "Export", rect(0.0, 0.0, 80.0, 30.0));
    first.test_id = Some("export".into());
    let mut second = control("n2", "button", "Export", rect(200.0, 0.0, 80.0, 30.0));
    second.test_id = Some("export".into());
    let found = checks(&snapshot(vec![first, second]));
    assert!(
        found.contains(&(
            UiCheck::DuplicateTestId,
            "testid:export".into(),
            Severity::Error
        )),
        "{found:?}"
    );
}

// ---------------------------------------------------------------------------
// Ambiguous interactive identity
// ---------------------------------------------------------------------------

#[test]
fn repeated_row_actions_in_separate_scopes_are_not_ambiguous() {
    let mut rows = Vec::new();
    for row in 1..=3 {
        let mut container = panel(
            &format!("row{row}"),
            rect(0.0, f64::from(row) * 40.0, 600.0, 40.0),
        );
        container.entity_key = Some(format!("order:{row}"));
        let mut button = control(
            &format!("del{row}"),
            "button",
            "Delete",
            rect(500.0, f64::from(row) * 40.0, 60.0, 30.0),
        );
        button.parent = Some(id(&format!("row{row}")));
        rows.push(container);
        rows.push(button);
    }
    let found = checks(&snapshot(rows));
    assert!(
        found.is_empty(),
        "three Delete buttons in three rows are three unambiguous controls: {found:?}"
    );
}

#[test]
fn two_save_buttons_in_one_dialog_are_ambiguous() {
    let mut dialog = panel("dialog", rect(100.0, 100.0, 400.0, 300.0));
    dialog.entity_key = Some("dialog:settings".into());
    let mut first = control("save1", "button", "Save", rect(120.0, 340.0, 80.0, 30.0));
    first.parent = Some(id("dialog"));
    let mut second = control("save2", "button", "Save", rect(220.0, 340.0, 80.0, 30.0));
    second.parent = Some(id("dialog"));
    let found = checks(&snapshot(vec![dialog, first, second]));
    assert!(
        found.contains(&(
            UiCheck::AmbiguousInteractive,
            "button:Save".into(),
            Severity::Error
        )),
        "{found:?}"
    );
}

#[test]
fn an_unresolvable_scope_warns_instead_of_blocking() {
    let first = control("save1", "button", "Save", rect(0.0, 0.0, 80.0, 30.0));
    let second = control("save2", "button", "Save", rect(200.0, 0.0, 80.0, 30.0));
    let found = checks(&snapshot(vec![first, second]));
    let severity = found
        .iter()
        .find(|(check, ..)| *check == UiCheck::AmbiguousInteractive)
        .map(|(.., severity)| *severity);
    assert_eq!(
        severity,
        Some(Severity::Warn),
        "with no row or dialog scope WVQ must not guess: {found:?}"
    );
}

// ---------------------------------------------------------------------------
// Interactive occlusion
// ---------------------------------------------------------------------------

#[test]
fn an_enabled_button_blocked_on_every_point_is_an_error() {
    let export = control("export", "button", "Export", rect(10.0, 10.0, 100.0, 40.0));
    let overlay = panel("overlay", rect(0.0, 0.0, 400.0, 400.0));
    let mut state = snapshot(vec![export, overlay]);
    state.hit_tests = fully_blocked("export", "overlay");
    let output = detect(&state, &enabled_policy()).unwrap();
    let finding = output
        .findings
        .iter()
        .find(|item| item.check == UiCheck::InteractiveOcclusion)
        .expect("occlusion must be reported");
    assert_eq!(finding.severity, Severity::Error);
    assert_eq!(finding.subject, "button:Export");
    assert_eq!(finding.evidence.sample_count, 9);
    assert_eq!(finding.evidence.received_event_samples, 0);
    assert_eq!(finding.evidence.failure_ratio_permille, 1000);
    assert_eq!(finding.viewport, "1280x720");
    assert_eq!(finding.route, "/checkout");
}

#[test]
fn a_reachable_button_is_not_occluded() {
    let export = control("export", "button", "Export", rect(10.0, 10.0, 100.0, 40.0));
    let mut state = snapshot(vec![export]);
    state.hit_tests = fully_reachable("export");
    assert!(checks(&state).is_empty());
}

#[test]
fn a_buttons_own_icon_on_top_of_it_is_not_occlusion() {
    let export = control("export", "button", "Export", rect(10.0, 10.0, 100.0, 40.0));
    let mut icon = panel("icon", rect(14.0, 14.0, 16.0, 16.0));
    icon.parent = Some(id("export"));
    let mut state = snapshot(vec![export, icon]);
    state.hit_tests = fully_blocked("export", "icon");
    assert!(
        checks(&state).is_empty(),
        "elementsFromPoint reporting a child is the normal case"
    );
}

#[test]
fn a_pointer_events_none_layer_never_occludes() {
    let export = control("export", "button", "Export", rect(10.0, 10.0, 100.0, 40.0));
    let mut ghost = panel("ghost", rect(0.0, 0.0, 400.0, 400.0));
    ghost.pointer_events = false;
    let mut state = snapshot(vec![export, ghost]);
    state.hit_tests = fully_blocked("export", "ghost");
    assert!(checks(&state).is_empty());
}

#[test]
fn a_disabled_control_is_not_reported_as_occluded() {
    let mut export = control("export", "button", "Export", rect(10.0, 10.0, 100.0, 40.0));
    export.enabled = false;
    let overlay = panel("overlay", rect(0.0, 0.0, 400.0, 400.0));
    let mut state = snapshot(vec![export, overlay]);
    state.hit_tests = fully_blocked("export", "overlay");
    assert!(
        checks(&state).is_empty(),
        "a disabled control was never going to receive the event"
    );
}

#[test]
fn losing_fewer_points_than_the_policy_threshold_is_not_occlusion() {
    let export = control("export", "button", "Export", rect(10.0, 10.0, 100.0, 40.0));
    let overlay = panel("overlay", rect(0.0, 0.0, 20.0, 20.0));
    let mut state = snapshot(vec![export, overlay]);
    // Two of nine points blocked: 222 permille, under the 500 default.
    state.hit_tests = fully_reachable("export");
    for sample in state.hit_tests.iter_mut().take(2) {
        sample.topmost = Some(id("overlay"));
    }
    assert!(checks(&state).is_empty());
}

#[test]
fn a_declared_tooltip_overlap_is_allowed() {
    let mut trigger = control("help", "button", "Help", rect(10.0, 10.0, 40.0, 40.0));
    trigger.role = Some("button".into());
    let mut tooltip = panel("tip", rect(0.0, 0.0, 200.0, 60.0));
    tooltip.role = Some("tooltip".into());
    let mut state = snapshot(vec![trigger, tooltip]);
    state.hit_tests = fully_blocked("help", "tip");

    let strict = detect(&state, &enabled_policy()).unwrap();
    assert!(
        strict
            .findings
            .iter()
            .any(|item| item.check == UiCheck::InteractiveOcclusion),
        "without an allowance a tooltip covering its trigger is still reported"
    );

    let policy = policy_with_overlap("tooltip", "button");
    let allowed = detect(&state, &policy).unwrap();
    assert!(
        allowed.findings.is_empty(),
        "a declared tooltip/button overlap is intentional: {:?}",
        allowed.findings
    );
}

fn policy_with_overlap(top_role: &str, bottom_role: &str) -> UiIntegrityPolicy {
    let yaml = format!(
        "enabled: true\nallowed_overlaps:\n  - top:\n      role: {top_role}\n    \
         bottom:\n      role: {bottom_role}\n    reason: intentional\n"
    );
    let value: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
    wvq_ui::parse_policy(&value, "2026-08-23").unwrap()
}

// ---------------------------------------------------------------------------
// Viewport overflow
// ---------------------------------------------------------------------------

#[test]
fn a_control_pushed_outside_the_viewport_is_an_error() {
    let button = control(
        "submit",
        "button",
        "Submit",
        rect(1240.0, 10.0, 200.0, 40.0),
    );
    let output = detect(&snapshot(vec![button]), &enabled_policy()).unwrap();
    let finding = output
        .findings
        .iter()
        .find(|item| item.check == UiCheck::ViewportOverflow)
        .expect("overflow must be reported");
    assert_eq!(finding.severity, Severity::Error);
    assert_eq!(finding.evidence.overflow_px, 160);
    assert!(finding.detail.contains("only partly reachable"));
}

#[test]
fn a_control_inside_a_scroll_container_is_not_overflowing() {
    let mut container = panel("scroller", rect(0.0, 0.0, 1280.0, 200.0));
    container.scrollable = true;
    let mut button = control(
        "submit",
        "button",
        "Submit",
        rect(1400.0, 10.0, 200.0, 40.0),
    );
    button.parent = Some(id("scroller"));
    assert!(
        checks(&snapshot(vec![container, button])).is_empty(),
        "content in a scroll container is reachable by scrolling"
    );
}

#[test]
fn whole_page_horizontal_overflow_is_a_warning_on_the_document() {
    let mut state = snapshot(vec![]);
    state.document.scroll_width = 1440.0;
    let found = checks(&state);
    assert!(
        found.contains(&(UiCheck::ViewportOverflow, "document".into(), Severity::Warn)),
        "{found:?}"
    );
}

// ---------------------------------------------------------------------------
// Text clipping
// ---------------------------------------------------------------------------

#[test]
fn clipped_body_text_is_a_warning() {
    let mut cell = panel("cell", rect(0.0, 0.0, 120.0, 20.0));
    cell.text_scroll_width = Some(310.0);
    cell.text_client_width = Some(120.0);
    cell.component_hint = Some("TableCell".into());
    let output = detect(&snapshot(vec![cell]), &enabled_policy()).unwrap();
    let finding = output
        .findings
        .iter()
        .find(|item| item.check == UiCheck::TextClipping)
        .expect("clipping must be reported");
    assert_eq!(finding.severity, Severity::Warn);
    assert_eq!(finding.evidence.scroll_width, 310);
    assert_eq!(finding.evidence.client_width, 120);
}

#[test]
fn an_accepted_ellipsis_with_an_accessible_full_value_is_allowed() {
    let mut cell = panel("cell", rect(0.0, 0.0, 120.0, 20.0));
    cell.text_scroll_width = Some(310.0);
    cell.text_client_width = Some(120.0);
    cell.component_hint = Some("TableCell".into());
    cell.accessible_name = Some("Quarterly revenue by product line".into());
    let yaml = "enabled: true\naccepted_text_truncation:\n  - target:\n      \
                component_hint: TableCell\n    requires_accessible_full_value: true\n";
    let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    let policy = wvq_ui::parse_policy(&value, "2026-08-23").unwrap();
    assert!(
        detect(&snapshot(vec![cell]), &policy)
            .unwrap()
            .findings
            .is_empty()
    );
}

#[test]
fn an_accepted_ellipsis_without_the_accessible_value_is_still_reported() {
    let mut cell = panel("cell", rect(0.0, 0.0, 120.0, 20.0));
    cell.text_scroll_width = Some(310.0);
    cell.text_client_width = Some(120.0);
    cell.component_hint = Some("TableCell".into());
    let yaml = "enabled: true\naccepted_text_truncation:\n  - target:\n      \
                component_hint: TableCell\n    requires_accessible_full_value: true\n";
    let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    let policy = wvq_ui::parse_policy(&value, "2026-08-23").unwrap();
    assert!(
        !detect(&snapshot(vec![cell]), &policy)
            .unwrap()
            .findings
            .is_empty(),
        "an ellipsis nobody can expand is not an accepted truncation"
    );
}

#[test]
fn a_clipped_critical_label_with_no_accessible_value_is_an_error() {
    let mut button = UiNode {
        id: id("pay"),
        role: Some("button".into()),
        rects: vec![rect(0.0, 0.0, 40.0, 30.0)],
        visible: true,
        interactive: true,
        enabled: true,
        pointer_events: true,
        text_scroll_width: Some(180.0),
        text_client_width: Some(40.0),
        ..UiNode::default()
    };
    button.dom_id = Some("pay".into());
    let output = detect(&snapshot(vec![button]), &enabled_policy()).unwrap();
    let finding = output
        .findings
        .iter()
        .find(|item| item.check == UiCheck::TextClipping)
        .expect("clipping must be reported");
    assert_eq!(finding.severity, Severity::Error);
    assert!(finding.detail.contains("no accessible name"));
}

#[test]
fn a_scroll_container_holding_more_than_it_shows_is_not_clipped_text() {
    let mut list = panel("list", rect(0.0, 0.0, 300.0, 200.0));
    list.scrollable = true;
    list.text_scroll_width = Some(300.0);
    list.text_client_width = Some(300.0);
    list.text_scroll_height = Some(2_400.0);
    list.text_client_height = Some(200.0);
    assert!(checks(&snapshot(vec![list])).is_empty());
}

// ---------------------------------------------------------------------------
// Forbidden overlap and its false positives
// ---------------------------------------------------------------------------

#[test]
fn a_child_inside_its_parent_is_never_a_forbidden_overlap() {
    let input = control("input", "textbox", "Email", rect(0.0, 0.0, 300.0, 40.0));
    let mut icon = control("clear", "button", "Clear", rect(260.0, 8.0, 24.0, 24.0));
    icon.parent = Some(id("input"));
    let mut state = snapshot(vec![input, icon]);
    state.hit_tests = fully_reachable("clear");
    assert!(
        !checks(&state)
            .iter()
            .any(|(check, ..)| *check == UiCheck::ForbiddenOverlap),
        "an icon button inside its input is containment"
    );
}

#[test]
fn geometric_overlap_without_hit_test_evidence_is_not_reported() {
    let first = control("a", "button", "One", rect(0.0, 0.0, 100.0, 40.0));
    let second = control("b", "button", "Two", rect(50.0, 0.0, 100.0, 40.0));
    let found = checks(&snapshot(vec![first, second]));
    assert!(
        !found
            .iter()
            .any(|(check, ..)| *check == UiCheck::ForbiddenOverlap),
        "geometry alone is not evidence: {found:?}"
    );
}

#[test]
fn a_confirmed_control_overlap_is_at_most_a_warning() {
    let first = control("a", "button", "One", rect(0.0, 0.0, 100.0, 40.0));
    let second = control("b", "button", "Two", rect(50.0, 0.0, 100.0, 40.0));
    let mut state = snapshot(vec![first, second]);
    // `Two` paints over `One`, but `One` still receives most of its points, so
    // the occlusion gate does not fire and this stays a warning.
    state.hit_tests = fully_reachable("a");
    state.hit_tests[0].topmost = Some(id("b"));
    let output = detect(&state, &enabled_policy()).unwrap();
    let overlap = output
        .findings
        .iter()
        .find(|item| item.check == UiCheck::ForbiddenOverlap)
        .expect("a confirmed control overlap must be reported");
    assert_eq!(overlap.severity, Severity::Warn);
    assert_eq!(overlap.subject, "button:One");
    assert_eq!(overlap.counterpart.as_deref(), Some("button:Two"));
    assert!(
        !output
            .findings
            .iter()
            .any(|item| item.check == UiCheck::InteractiveOcclusion),
        "one blocked point out of nine is not an occlusion"
    );
}

#[test]
fn a_badge_over_an_avatar_is_allowed_when_declared() {
    let mut avatar = control("avatar", "img", "Profile", rect(0.0, 0.0, 48.0, 48.0));
    avatar.component_hint = Some("Avatar".into());
    let mut badge = control("badge", "status", "3 unread", rect(32.0, 0.0, 20.0, 20.0));
    badge.component_hint = Some("Badge".into());
    let mut state = snapshot(vec![avatar, badge]);
    state.hit_tests = fully_reachable("avatar");
    state.hit_tests[0].topmost = Some(id("badge"));

    let yaml = "enabled: true\nallowed_overlaps:\n  - top:\n      component_hint: Badge\n    \
                bottom:\n      component_hint: Avatar\n    reason: design system\n";
    let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    let policy = wvq_ui::parse_policy(&value, "2026-08-23").unwrap();
    let found = detect(&state, &policy).unwrap();
    assert!(
        !found
            .findings
            .iter()
            .any(|item| item.check == UiCheck::ForbiddenOverlap),
        "{:?}",
        found.findings
    );
}

#[test]
fn a_dialog_over_its_backdrop_is_not_a_control_overlap() {
    let backdrop = panel("backdrop", rect(0.0, 0.0, 1280.0, 720.0));
    let dialog = panel("dialog", rect(300.0, 200.0, 600.0, 300.0));
    let confirm = control("ok", "button", "OK", rect(700.0, 440.0, 80.0, 30.0));
    let mut state = snapshot(vec![backdrop, dialog, confirm]);
    state.hit_tests = fully_reachable("ok");
    assert!(
        checks(&state).is_empty(),
        "a backdrop is not an interactive control"
    );
}

#[test]
fn a_sticky_header_that_does_not_cover_the_target_is_clean() {
    let mut header = control("nav", "button", "Menu", rect(0.0, 0.0, 1280.0, 60.0));
    header.position = Some("sticky".into());
    let target = control("buy", "button", "Buy", rect(100.0, 300.0, 120.0, 40.0));
    let mut state = snapshot(vec![header, target]);
    state.hit_tests = fully_reachable("buy");
    assert!(checks(&state).is_empty());
}

// ---------------------------------------------------------------------------
// Boundedness
// ---------------------------------------------------------------------------

#[test]
fn a_truncated_snapshot_is_never_reported_as_clean() {
    let mut state = snapshot(vec![control(
        "ok",
        "button",
        "OK",
        rect(0.0, 0.0, 40.0, 20.0),
    )]);
    state.truncated = true;
    let output = detect(&state, &enabled_policy()).unwrap();
    assert!(output.findings.is_empty());
    assert!(
        output.truncated,
        "truncation must survive into the delta and the verdict"
    );
}

#[test]
fn exceeding_the_configured_node_ceiling_marks_the_snapshot_truncated() {
    let nodes = (0..12)
        .map(|index| {
            control(
                &format!("n{index}"),
                "button",
                &format!("Item {index}"),
                rect(f64::from(index) * 100.0, 0.0, 80.0, 30.0),
            )
        })
        .collect();
    let policy = UiIntegrityPolicy {
        enabled: true,
        max_nodes: 10,
        ..UiIntegrityPolicy::default()
    };
    assert!(detect(&snapshot(nodes), &policy).unwrap().truncated);
}

#[test]
fn a_disabled_policy_produces_no_findings() {
    let mut first = control("n1", "button", "Save", rect(0.0, 0.0, 80.0, 30.0));
    first.dom_id = Some("save".into());
    let mut second = control("n2", "button", "Save", rect(200.0, 0.0, 80.0, 30.0));
    second.dom_id = Some("save".into());
    let output = detect(
        &snapshot(vec![first, second]),
        &UiIntegrityPolicy::default(),
    )
    .unwrap();
    assert!(output.findings.is_empty());
}

// ---------------------------------------------------------------------------
// Snapshot validation
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_snapshot_schema_fails_closed() {
    let mut state = snapshot(vec![]);
    state.schema_v = 3;
    assert!(matches!(
        detect(&state, &enabled_policy()),
        Err(wvq_ui::UiError::UnknownSchema(3))
    ));
}

#[test]
fn a_dangling_parent_reference_fails_closed() {
    let mut button = control("ok", "button", "OK", rect(0.0, 0.0, 40.0, 20.0));
    button.parent = Some(id("missing"));
    assert!(matches!(
        detect(&snapshot(vec![button]), &enabled_policy()),
        Err(wvq_ui::UiError::Malformed(_))
    ));
}

#[test]
fn an_unbounded_accessible_name_fails_closed() {
    let button = control("ok", "button", &"x".repeat(400), rect(0.0, 0.0, 40.0, 20.0));
    let err = detect(&snapshot(vec![button]), &enabled_policy()).unwrap_err();
    assert!(
        matches!(&err, wvq_ui::UiError::Malformed(message) if message.contains("redact")),
        "{err:?}"
    );
}

#[test]
fn detection_is_deterministic() {
    let mut first = control("n1", "button", "Save", rect(0.0, 0.0, 80.0, 30.0));
    first.dom_id = Some("save".into());
    let mut second = control("n2", "button", "Save", rect(200.0, 0.0, 80.0, 30.0));
    second.dom_id = Some("save".into());
    let state = snapshot(vec![first, second]);
    assert_eq!(
        detect(&state, &enabled_policy()).unwrap(),
        detect(&state, &enabled_policy()).unwrap()
    );
}
