//! Diagnostic failure reel: green path stays empty, frames stay bounded.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use wvq_runtime::{
    FailureCauseKind, FailureReelCapture, MAX_FAILURE_REEL_CAUSE_CHARS, MAX_FAILURE_REEL_FRAME_BYTES,
    MAX_FAILURE_REEL_FRAMES, Target, assemble_failure_reel, copy_reel_frame, failure_cause,
    summarize_target,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("wvq-failure-reel-{nanos}"));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn capture(dir: &TempDir) -> FailureReelCapture {
    FailureReelCapture {
        program: "checkout-widget".into(),
        step: 2,
        action: "activate".into(),
        target: Some(Target {
            role: Some("button".into()),
            accessible_name: Some("Pay".into()),
            ..Target::default()
        }),
        failure: "assertion_failed:payment-visible:sealed expectation visible was not met".into(),
        before_path: Some(dir.write("before.png", b"before-png")),
        highlight_path: Some(dir.write("highlight.png", b"highlight-png")),
        after_path: Some(dir.write("after.png", b"after-png")),
        limitations: Vec::new(),
    }
}

#[test]
fn a_passing_program_does_not_assemble_a_failure_reel() {
    let dir = TempDir::new();
    let captured = capture(&dir);
    assert!(assemble_failure_reel(true, Some(&captured)).is_none());
}

#[test]
fn a_failed_step_assembles_a_diagnostic_reel_that_is_never_a_verdict_source() {
    let dir = TempDir::new();
    let captured = capture(&dir);
    let reel = assemble_failure_reel(false, Some(&captured)).expect("failed run must assemble");

    assert!(reel.diagnostic, "reviewers and join_triangle must ignore this artifact");
    assert_eq!(reel.runtime_llm_tokens, 0);
    assert_eq!(reel.schema_v, 1);
    assert_eq!(reel.program, "checkout-widget");
    assert_eq!(reel.step, 2);
    assert_eq!(reel.action, "activate");
    assert_eq!(reel.target.as_deref(), Some("button:Pay"));
    assert_eq!(reel.cause.kind, FailureCauseKind::Assertion);
    assert_eq!(reel.cause.obligation.as_deref(), Some("payment-visible"));
    assert_eq!(reel.frames.count(), 3);
    assert!(reel.frames.count() <= MAX_FAILURE_REEL_FRAMES);
    assert_eq!(reel.frames.before.as_deref(), Some("before.png"));
    assert_eq!(reel.frames.highlight.as_deref(), Some("highlight.png"));
    assert_eq!(reel.frames.after.as_deref(), Some("after.png"));
}

#[test]
fn missing_before_and_highlight_are_limitations_not_invented_frames() {
    let dir = TempDir::new();
    let mut captured = capture(&dir);
    captured.before_path = None;
    captured.highlight_path = None;
    captured.limitations = vec!["before_frame_unmeasured".into(), "target_not_located".into()];

    let reel = assemble_failure_reel(false, Some(&captured)).unwrap();
    assert!(reel.frames.before.is_none());
    assert!(reel.frames.highlight.is_none());
    assert_eq!(reel.frames.after.as_deref(), Some("after.png"));
    assert!(reel.limitations.iter().any(|item| item == "before_frame_unmeasured"));
    assert!(reel.limitations.iter().any(|item| item == "target_not_located"));
}

#[test]
fn a_navigate_failure_records_that_no_semantic_target_applied() {
    let dir = TempDir::new();
    let captured = FailureReelCapture {
        program: "open-home".into(),
        step: 0,
        action: "navigate".into(),
        target: None,
        failure: "Timeout 15000ms exceeded".into(),
        before_path: None,
        highlight_path: None,
        after_path: Some(dir.write("after.png", b"after")),
        limitations: Vec::new(),
    };
    let reel = assemble_failure_reel(false, Some(&captured)).unwrap();
    assert_eq!(reel.cause.kind, FailureCauseKind::Action);
    assert!(reel.limitations.iter().any(|item| item == "target_not_applicable"));
    assert!(reel.limitations.iter().any(|item| item == "before_frame_unmeasured"));
}

#[test]
fn geometric_predicates_become_a_geometric_cause() {
    let cause = failure_cause(
        "assertion_failed:save-visible:sealed expectation no_overlap was not met",
    );
    assert_eq!(cause.kind, FailureCauseKind::Geometric);
    assert_eq!(cause.obligation.as_deref(), Some("save-visible"));
    assert_eq!(cause.check.as_deref(), Some("no_overlap"));
}

#[test]
fn oversized_frames_are_dropped_with_a_bound_limitation() {
    let dir = TempDir::new();
    let mut captured = capture(&dir);
    let huge = dir.0.join("after.png");
    let file = std::fs::File::create(&huge).unwrap();
    file.set_len(MAX_FAILURE_REEL_FRAME_BYTES.saturating_add(1))
        .unwrap();
    captured.after_path = Some(huge);

    let reel = assemble_failure_reel(false, Some(&captured)).unwrap();
    assert!(reel.frames.after.is_none());
    assert!(
        reel.limitations
            .iter()
            .any(|item| item == "frame_exceeded_bound:after")
    );
}

#[test]
fn cause_text_is_truncated_to_the_bound() {
    let long = format!("assertion_failed:pay:{}", "x".repeat(MAX_FAILURE_REEL_CAUSE_CHARS + 40));
    let cause = failure_cause(&long);
    assert_eq!(cause.text.chars().count(), MAX_FAILURE_REEL_CAUSE_CHARS);
    assert_eq!(cause.kind, FailureCauseKind::Assertion);
}

#[test]
fn a_before_frame_is_copied_to_a_reel_owned_name() {
    let dir = TempDir::new();
    let source = dir.write("observation.png", b"observation-bytes");
    let copied = copy_reel_frame(&source, &dir.0, "checkout widget", 1, "before").unwrap();
    assert_eq!(
        copied.file_name().unwrap().to_string_lossy(),
        "checkout-widget-reel-1-before.png"
    );
    assert_eq!(std::fs::read(&copied).unwrap(), b"observation-bytes");
    assert_eq!(std::fs::read(&source).unwrap(), b"observation-bytes");
}

#[test]
fn target_summary_prefers_test_id_then_role_name() {
    assert_eq!(
        summarize_target(&Target {
            test_id: Some("pay".into()),
            role: Some("button".into()),
            accessible_name: Some("Pay now".into()),
            ..Target::default()
        }),
        "testid:pay"
    );
    assert_eq!(
        summarize_target(&Target {
            role: Some("button".into()),
            accessible_name: Some("Pay now".into()),
            ..Target::default()
        }),
        "button:Pay now"
    );
}

#[test]
fn empty_capture_does_not_assemble() {
    assert!(assemble_failure_reel(false, None).is_none());
    let captured = FailureReelCapture {
        program: "   ".into(),
        ..FailureReelCapture::default()
    };
    assert!(assemble_failure_reel(false, Some(&captured)).is_none());
}
