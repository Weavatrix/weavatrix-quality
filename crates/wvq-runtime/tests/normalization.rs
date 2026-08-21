//! Task 10: `JUnit`, LCOV, and `go test -json` normalize or fail closed.

use std::path::{Path, PathBuf};

use wvq_runtime::{
    TestStatus, parse_cargo_test, parse_go_coverprofile, parse_go_json, parse_junit, parse_lcov,
};

fn fixture(parts: &[&str]) -> PathBuf {
    let mut path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures");
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
fn cargo_test_output_binds_cases_to_the_executed_test_target() {
    let stderr = "   Compiling demo v0.1.0\n\u{1b}[1m\u{1b}[32m     Running\u{1b}[0m unittests src/lib.rs (target/debug/deps/demo)\n\u{1b}[1m\u{1b}[32m     Running\u{1b}[0m tests/permission.rs (target/debug/deps/permission)\n";
    let stdout = "\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored\n\nrunning 3 tests\ntest admin_can_delete ... \u{1b}[32mok\u{1b}[0m\ntest viewer_cannot_delete ... ok\ntest future_role ... ignored\n\ntest result: ok. 2 passed; 0 failed; 1 ignored\n";
    let run = parse_cargo_test(stdout, stderr).unwrap();
    assert_eq!(run.cases.len(), 3);
    assert_eq!(run.cases[0].suite, "tests/permission.rs");
    assert_eq!(run.cases[0].name, "admin_can_delete");
    assert_eq!(run.cases[1].name, "viewer_cannot_delete");
    assert_eq!(run.cases[1].status, TestStatus::Pass);
    assert_eq!(run.cases[2].status, TestStatus::Skip);
}

#[test]
fn go_coverprofile_maps_measured_ranges() {
    let coverage = parse_go_coverprofile(&read(&["go", "coverage.out"])).unwrap();
    assert_eq!(coverage.files.len(), 1);
    assert_eq!(coverage.files[0].path, "example.com/wvq/add.go");
    assert_eq!(coverage.files[0].covered, [range(1, 3)]);
    assert_eq!(coverage.files[0].uncovered, [range(5, 7)]);
}

#[test]
fn malformed_and_truncated_evidence_is_rejected() {
    assert!(parse_junit("<testsuites><testcase").is_err());
    assert!(parse_junit("<testsuites><testcase time=\"1\"/></testsuites>").is_err());
    assert!(parse_lcov("DA:1,1\nend_of_record\n").is_err());
    assert!(parse_lcov("SF:src/a.js\nDA:1,1\n").is_err());
    assert!(
        parse_go_json("{\"Action\":\"run\",\"Package\":\"p\",\"Test\":\"T\"}\n{\"Action\":\"run\"")
            .is_err()
    );
    assert!(
        parse_go_json("{\"Action\":\"run\",\"Package\":\"p\",\"Test\":\"NeverFinishes\"}\n")
            .is_err()
    );
    assert!(parse_go_coverprofile("mode: set\n").is_err());
    assert!(parse_go_coverprofile("not-a-mode\nfile.go:1.1,2.1 1 1\n").is_err());
}

fn range(start: u32, end: u32) -> wvq_runtime::LineRange {
    wvq_runtime::LineRange { start, end }
}
