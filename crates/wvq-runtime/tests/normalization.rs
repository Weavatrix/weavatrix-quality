//! Task 10: `JUnit`, LCOV, and `go test -json` normalize or fail closed.

use std::path::{Path, PathBuf};

use wvq_runtime::{TestStatus, parse_go_json, parse_junit, parse_lcov};

fn fixture(parts: &[&str]) -> PathBuf {
    let mut path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join("fixtures");
    for part in parts {
        path.push(part);
    }
    path
}

fn read(parts: &[&str]) -> String {
    std::fs::read_to_string(fixture(parts)).expect("fixture readable")
}

#[test]
fn vitest_junit_parses_pass_skip_fail() {
    let run = parse_junit(&read(&["ts-vitest", "junit.xml"])).unwrap();
    assert_eq!(run.cases.len(), 3);
    assert_eq!(run.cases[0].status, TestStatus::Pass);
    assert_eq!(run.cases[0].name, "adds two numbers");
    assert_eq!(run.cases[1].status, TestStatus::Skip);
    assert_eq!(run.cases[2].status, TestStatus::Fail);
    assert_eq!(run.cases[2].duration_ms, Some(28));
    assert!(
        run.cases[2]
            .message
            .as_ref()
            .is_some_and(|msg| msg.contains("expected 1 to be 2"))
    );
}

#[test]
fn bun_junit_and_lcov_fixtures() {
    let run = parse_junit(&read(&["bun", "junit.xml"])).unwrap();
    assert_eq!(run.cases.len(), 2);
    assert!(run.cases.iter().all(|case| case.status == TestStatus::Pass));
    let coverage = parse_lcov(&read(&["bun", "coverage.lcov"])).unwrap();
    assert_eq!(coverage.files[0].path, "src/add.js");
    assert_eq!(coverage.files[0].covered.len(), 2);
}

#[test]
fn lcov_maps_consecutive_lines_to_ranges() {
    let coverage = parse_lcov(&read(&["ts-vitest", "coverage.lcov"])).unwrap();
    let file = &coverage.files[0];
    assert_eq!(file.path, "src/add.js");
    assert_eq!(file.covered, [range(1, 2), range(5, 5)]);
    assert_eq!(file.uncovered, [range(3, 4)]);
}

#[test]
fn go_json_parses_pass_fail_skip() {
    let run = parse_go_json(&read(&["go", "test.jsonl"])).unwrap();
    assert_eq!(run.cases.len(), 3);
    assert_eq!(run.cases[0].status, TestStatus::Pass);
    assert_eq!(run.cases[1].status, TestStatus::Fail);
    assert_eq!(run.cases[2].status, TestStatus::Skip);
    assert!(
        run.cases[1]
            .message
            .as_ref()
            .is_some_and(|msg| msg.contains("overflow"))
    );
}

#[test]
fn malformed_and_truncated_evidence_is_rejected() {
    assert!(parse_junit("<testsuites><testcase").is_err());
    assert!(parse_junit("<testsuites><testcase time=\"1\"/></testsuites>").is_err());
    assert!(parse_lcov("DA:1,1\nend_of_record\n").is_err());
    assert!(parse_lcov("SF:src/a.js\nDA:1,1\n").is_err());
    assert!(parse_go_json("{\"Action\":\"run\",\"Package\":\"p\",\"Test\":\"T\"}\n{\"Action\":\"run\"").is_err());
    assert!(parse_go_json("{\"Action\":\"run\",\"Package\":\"p\",\"Test\":\"NeverFinishes\"}\n").is_err());
}

fn range(start: u32, end: u32) -> wvq_runtime::LineRange {
    wvq_runtime::LineRange { start, end }
}
