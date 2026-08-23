//! The UI-integrity policy must fail closed. A typo may not silence a gate.

use wvq_ui::{DEFAULT_OCCLUSION_FAILURE_PERMILLE, UiError, parse_policy};

fn parse(yaml: &str) -> Result<wvq_ui::UiIntegrityPolicy, UiError> {
    let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    parse_policy(&value, "2026-08-23")
}

fn message(yaml: &str) -> String {
    match parse(yaml) {
        Err(UiError::Policy(message)) => message,
        other => panic!("expected a policy error, got {other:?}"),
    }
}

#[test]
fn the_documented_example_parses() {
    let policy = parse(
        "enabled: true\n\
         max_nodes: 5000\n\
         geometry_tolerance_px: 1\n\
         occlusion_failure_ratio: 0.5\n\
         allowed_overlaps:\n\
         \x20 - top:\n\
         \x20     role: tooltip\n\
         \x20   bottom:\n\
         \x20     role: button\n\
         \x20 - top:\n\
         \x20     component_hint: Badge\n\
         \x20   bottom:\n\
         \x20     component_hint: Avatar\n\
         accepted_text_truncation:\n\
         \x20 - target:\n\
         \x20     component_hint: TableCell\n\
         \x20   requires_accessible_full_value: true\n",
    )
    .unwrap();
    assert!(policy.enabled);
    assert_eq!(policy.max_nodes, 5_000);
    assert_eq!(policy.geometry_tolerance_px, 1);
    assert_eq!(policy.occlusion_failure_permille, 500);
    assert_eq!(policy.allowed_overlaps.len(), 2);
    assert_eq!(policy.accepted_text_truncation.len(), 1);
}

#[test]
fn defaults_apply_when_only_enabled_is_given() {
    let policy = parse("enabled: true\n").unwrap();
    assert_eq!(policy.max_nodes, wvq_ui::DEFAULT_MAX_NODES);
    assert_eq!(
        policy.occlusion_failure_permille,
        DEFAULT_OCCLUSION_FAILURE_PERMILLE
    );
}

#[test]
fn an_unknown_field_fails_closed() {
    assert!(message("enabled: true\nmax_node: 10\n").contains("unknown field `max_node`"));
}

#[test]
fn an_unknown_matcher_field_fails_closed() {
    let yaml = "enabled: true\nallowed_overlaps:\n  - top:\n      selector: .tip\n    \
                bottom:\n      role: button\n";
    assert!(message(yaml).contains("unknown field `selector`"));
}

#[test]
fn accept_all_is_refused_by_name() {
    assert!(message("enabled: true\naccept_all: true\n").contains("may not accept everything"));
}

#[test]
fn an_empty_matcher_is_refused() {
    let yaml = "enabled: true\nallowed_overlaps:\n  - top: {}\n    bottom:\n      role: button\n";
    assert!(message(yaml).contains("names no node"));
}

#[test]
fn a_missing_matcher_side_is_refused() {
    let yaml = "enabled: true\nallowed_overlaps:\n  - top:\n      role: tooltip\n";
    assert!(message(yaml).contains("requires `bottom`"));
}

#[test]
fn a_path_shaped_matcher_value_is_refused() {
    let yaml = "enabled: true\nallowed_overlaps:\n  - top:\n      \
                component_hint: ../../etc/passwd\n    bottom:\n      role: button\n";
    assert!(message(yaml).contains("not a path"));
}

#[test]
fn a_ratio_outside_zero_to_one_is_refused() {
    assert!(
        message("enabled: true\nocclusion_failure_ratio: 1.5\n").contains("between 0.0 and 1.0")
    );
    assert!(
        message("enabled: true\nocclusion_failure_ratio: -0.2\n").contains("between 0.0 and 1.0")
    );
}

#[test]
fn a_non_numeric_ratio_is_refused() {
    assert!(message("enabled: true\nocclusion_failure_ratio: half\n").contains("must be a number"));
}

#[test]
fn an_out_of_range_tolerance_is_refused() {
    assert!(message("enabled: true\ngeometry_tolerance_px: 999\n").contains("between 0 and 64"));
}

#[test]
fn a_node_ceiling_above_the_hard_bound_is_refused() {
    let yaml = format!("enabled: true\nmax_nodes: {}\n", wvq_ui::MAX_NODES + 1);
    assert!(message(&yaml).contains("must be an integer between"));
}

#[test]
fn an_exception_without_a_reason_is_refused() {
    let yaml = "enabled: true\nexceptions:\n  - fingerprint: ui:WVQ-UI-DUP-001:abc\n";
    assert!(message(yaml).contains("requires a non-empty `reason`"));
}

#[test]
fn an_exception_for_a_foreign_fingerprint_is_refused() {
    let yaml = "enabled: true\nexceptions:\n  - fingerprint: WVQ-ARCH-001\n    reason: legacy\n";
    assert!(message(yaml).contains("not a UI-integrity fingerprint"));
}

#[test]
fn a_malformed_expiry_is_refused() {
    let yaml = "enabled: true\nexceptions:\n  - fingerprint: ui:WVQ-UI-DUP-001:abc\n    \
                reason: legacy\n    expires: soon\n";
    assert!(message(yaml).contains("ISO `YYYY-MM-DD`"));
}

#[test]
fn an_expired_exception_stops_applying_and_stays_visible() {
    let policy = parse(
        "enabled: true\nexceptions:\n  - fingerprint: ui:WVQ-UI-DUP-001:abc\n    \
         reason: legacy\n    expires: 2020-01-01\n",
    )
    .unwrap();
    assert!(policy.exceptions.is_empty());
    assert_eq!(policy.expired.len(), 1);
    assert!(policy.expired[0].contains("expired 2020-01-01"));
}

#[test]
fn an_unexpired_exception_still_applies() {
    let policy = parse(
        "enabled: true\nexceptions:\n  - fingerprint: ui:WVQ-UI-DUP-001:abc\n    \
         reason: legacy\n    expires: 2099-01-01\n",
    )
    .unwrap();
    assert_eq!(policy.exceptions.len(), 1);
    assert!(policy.expired.is_empty());
}

#[test]
fn an_expired_allowed_overlap_stops_applying() {
    let policy = parse(
        "enabled: true\nallowed_overlaps:\n  - top:\n      role: tooltip\n    \
         bottom:\n      role: button\n    reason: temporary\n    expires: 2020-01-01\n",
    )
    .unwrap();
    assert!(policy.allowed_overlaps.is_empty());
    assert_eq!(policy.expired.len(), 1);
}

#[test]
fn a_non_mapping_section_is_refused() {
    assert!(message("- enabled\n").contains("must be a mapping"));
}

#[test]
fn a_non_list_allowance_is_refused() {
    assert!(message("enabled: true\nallowed_overlaps: yes\n").contains("must be a list"));
}
