//! Runner artifact admission: generated reports, freshness, and JUnit failures.

use super::*;

#[test]
fn generated_runner_report_is_cleared_without_touching_user_report_paths() {
    let root = TempDir::new("clear-runner-artifacts");
    std::fs::create_dir_all(root.0.join(".weavatrix-quality")).unwrap();
    let generated = root.0.join(".weavatrix-quality/junit.xml");
    let user_owned = root.0.join("junit.xml");
    std::fs::write(&generated, "stale generated evidence").unwrap();
    std::fs::write(&user_owned, "repository-owned evidence").unwrap();

    clear_generated_runner_artifacts(&root.0).unwrap();

    assert!(!generated.exists());
    assert_eq!(
        std::fs::read_to_string(user_owned).unwrap(),
        "repository-owned evidence"
    );
}

#[test]
fn malformed_fresh_evidence_fails_the_record() {
    let root = TempDir::new("bad-runner-artifact");
    let started = SystemTime::now();
    std::fs::write(root.0.join("junit.xml"), "<testsuite><testcase").unwrap();
    let mut record = record("npm-test");

    attach_normalized_artifacts(&root.0, &root.0, started, &mut record);

    assert!(!record.passed);
    assert!(
        record
            .error
            .as_deref()
            .is_some_and(|message| message.contains("truncated junit"))
    );
    assert_eq!(record.artifacts.len(), 1, "raw evidence remains auditable");
}

#[test]
fn failed_junit_fails_the_record_even_when_the_process_exits_zero() {
    let root = TempDir::new("failed-junit-zero-exit");
    let started = SystemTime::now();
    std::fs::write(
        root.0.join("junit.xml"),
        "<testsuite name=\"suite\" failures=\"1\"><testcase name=\"fails\"><failure message=\"boom\"/></testcase></testsuite>",
    )
    .unwrap();
    let mut record = record("storybook-vitest-v8");

    attach_normalized_artifacts(&root.0, &root.0, started, &mut record);

    assert!(!record.passed);
    assert!(record.error.as_deref().is_some_and(|message| {
        message.contains("reports 1 failed or errored test case")
    }));
    assert!(
        record
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == "normalized-test-run")
    );
}

#[test]
fn artifact_from_before_the_run_is_not_reused() {
    let root = TempDir::new("stale-runner-artifact");
    std::fs::write(
        root.0.join("junit.xml"),
        "<testsuite><testcase name=\"stale\"/></testsuite>",
    )
    .unwrap();
    let started = SystemTime::now() + Duration::from_secs(10);
    let mut record = record("npm-test");

    attach_normalized_artifacts(&root.0, &root.0, started, &mut record);

    assert!(record.artifacts.is_empty());
}
