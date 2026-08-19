//! Task 17: shadow evaluation of selected vs full. No published 10× claim.

use std::path::{Path, PathBuf};

use wvq_bench::{
    Ecosystem, ShadowCase, evaluate, go_service_case, node_bun_backend_case,
    ten_x_publication_blocked_reason, ts_frontend_case,
};
use wvq_runtime::{parse_go_json, parse_junit};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn assert_shadow_invariants(case: &ShadowCase) {
    let report = evaluate(case);
    assert!(
        report.selected_count <= report.full_count,
        "{} selected more tests than the full suite",
        case.name
    );
    assert!(
        report.selected_wall_clock_ms <= report.full_wall_clock_ms,
        "{} selected wall-clock exceeded full",
        case.name
    );
    assert_eq!(
        report.runtime_tokens, 0,
        "green path must spend 0 runtime LLM tokens"
    );
    assert_eq!(report.algorithm, "greedy-weighted-set-cover");
    assert!(
        report.ten_x_blocked.is_some(),
        "default cases must not allow a 10× publication"
    );
    let summary = report.summary_line();
    assert!(
        !summary.to_ascii_lowercase().contains("10x") && !summary.contains('×'),
        "summary must not publish a 10× claim: {summary}"
    );
}

#[test]
fn evaluates_ts_frontend() {
    let case = ts_frontend_case();
    assert_eq!(case.ecosystem, Ecosystem::TsFrontend);
    let report = evaluate(&case);
    assert_shadow_invariants(&case);
    assert!(report.selected_count < report.full_count);
    assert_eq!(report.bugs_recovered, 1);
    assert_eq!(report.false_positives, 1);
    assert_eq!(report.false_negatives, 0);
    assert!(report.artifact_bytes > 0);
}

#[test]
fn evaluates_node_bun_backend() {
    let case = node_bun_backend_case();
    assert_eq!(case.ecosystem, Ecosystem::NodeBunBackend);
    let report = evaluate(&case);
    assert_shadow_invariants(&case);
    assert!(report.selected_count < report.full_count);
    assert_eq!(report.bugs_recovered, 1);
    assert_eq!(report.false_positives, 0);
    assert_eq!(report.false_negatives, 1);
}

#[test]
fn evaluates_go_service() {
    let case = go_service_case();
    assert_eq!(case.ecosystem, Ecosystem::GoService);
    let report = evaluate(&case);
    assert_shadow_invariants(&case);
    assert!(report.selected_count < report.full_count);
    assert_eq!(report.bugs_recovered, 1);
    assert_eq!(report.false_positives, 0);
    assert_eq!(report.false_negatives, 0);
}

#[test]
fn ten_x_is_blocked_without_human_touch_time() {
    let case = ts_frontend_case();
    assert_eq!(
        ten_x_publication_blocked_reason(&case),
        Some("human-touch-time data is missing")
    );
}

#[test]
fn ten_x_is_blocked_when_escaped_regressions_increase() {
    let mut case = ts_frontend_case();
    case.human_touch_minutes = Some(6);
    case.baseline_human_touch_minutes = Some(60);
    case.escaped_regressions_delta = 1;
    assert_eq!(
        ten_x_publication_blocked_reason(&case),
        Some("escaped regressions increased")
    );
    let report = evaluate(&case);
    assert!(report.ten_x_blocked.is_some());
    assert!(!report.summary_line().to_ascii_lowercase().contains("10x"));
}

#[test]
fn ten_x_gate_opens_only_with_human_and_safety_data() {
    let mut case = ts_frontend_case();
    case.human_touch_minutes = Some(6);
    case.baseline_human_touch_minutes = Some(60);
    case.escaped_regressions_delta = 0;
    assert_eq!(ten_x_publication_blocked_reason(&case), None);
    let report = evaluate(&case);
    assert!(report.ten_x_blocked.is_none());
    assert!(
        !report.summary_line().to_ascii_lowercase().contains("10x"),
        "an open gate still must not print a 10× headline"
    );
}

#[test]
fn vitest_fixture_is_the_ts_frontend_full_suite_seed() {
    let xml = std::fs::read_to_string(repo_root().join("fixtures/ts-vitest/junit.xml")).unwrap();
    let run = parse_junit(&xml).unwrap();
    assert_eq!(
        run.cases.len(),
        3,
        "TS frontend fixture must stay measurable"
    );
    let wall: u64 = run.cases.iter().filter_map(|item| item.duration_ms).sum();
    assert!(wall > 0);
}

#[test]
fn bun_fixture_is_the_node_backend_full_suite_seed() {
    let xml = std::fs::read_to_string(repo_root().join("fixtures/bun/junit.xml")).unwrap();
    let run = parse_junit(&xml).unwrap();
    assert_eq!(run.cases.len(), 2);
}

#[test]
fn go_json_fixture_is_the_go_service_full_suite_seed() {
    let raw = std::fs::read_to_string(repo_root().join("fixtures/go/test.jsonl")).unwrap();
    let run = parse_go_json(&raw).unwrap();
    assert!(run.cases.len() >= 3);
}
