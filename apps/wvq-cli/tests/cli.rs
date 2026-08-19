//! Task 16: CLI maps onto the command bus; blocking verdict is non-zero.

use std::path::{Path, PathBuf};

use wvq_cli::run_with;
use wvq_command_bus::FakeService;

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_owned()).collect()
}

fn fixture_repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("openspec")
        .join("repo")
}

#[test]
fn spec_validate_succeeds_on_fixture() {
    let repo = fixture_repo();
    let output = wvq_cli::run(&argv(&[
        "--repo",
        repo.to_str().expect("utf-8 path"),
        "spec",
        "validate",
        "--change",
        "sankey-others",
    ]));
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert!(output.stdout.contains("spec_validate"));
    assert!(output.stdout.contains("sankey-others"));
}

#[test]
fn spec_seal_prints_oracle_seal_id() {
    let repo = fixture_repo();
    let output = wvq_cli::run(&argv(&[
        "--repo",
        repo.to_str().expect("utf-8 path"),
        "spec",
        "seal",
        "--change",
        "sankey-others",
    ]));
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert!(output.stdout.contains("oseal-"));
}

#[test]
fn analyze_debt_select_run_verify_explain_are_wired() {
    let fake = FakeService::default();
    fake.set_verdict("UNPROVEN");
    fake.put_explain(wvq_command_bus::ExplainReply {
        id: "others-visible".into(),
        kind: "obligation".into(),
        summary: "others stay visible".into(),
        provenance: vec!["quality.yaml:1".into()],
    });
    for cmd in ["analyze", "debt", "select", "run", "verify"] {
        let output = run_with(&argv(&[cmd, "--change", "sankey-others"]), &fake);
        assert_ne!(
            output.code, 2,
            "{cmd} must not be a usage error: {}",
            output.stderr
        );
        assert!(
            output.stderr.is_empty() || output.code != 0,
            "{cmd}: {}",
            output.stderr
        );
    }
    let explained = run_with(&argv(&["explain", "others-visible"]), &fake);
    assert_eq!(explained.code, 0, "{}", explained.stderr);
    assert!(explained.stdout.contains("others stay visible"));
}

#[test]
fn block_verdict_returns_nonzero() {
    let fake = FakeService::default();
    fake.set_verdict("CONTRADICTED");
    let output = run_with(&argv(&["verify", "--change", "sankey-others"]), &fake);
    assert_eq!(output.code, 2, "blocking verdict must fail CI");
    assert!(output.stdout.contains("CONTRADICTED"));
}

#[test]
fn proven_verify_is_zero() {
    let fake = FakeService::default();
    fake.set_verdict("PROVEN");
    let output = run_with(&argv(&["verify", "--change", "sankey-others"]), &fake);
    assert_eq!(output.code, 0, "{}", output.stderr);
}

#[test]
fn unknown_command_is_nonzero() {
    let fake = FakeService::default();
    let output = run_with(&argv(&["mutate"]), &fake);
    assert_ne!(output.code, 0);
    assert!(output.stderr.contains("unknown command"));
}

#[test]
fn explain_requires_id() {
    let fake = FakeService::default();
    let output = run_with(&argv(&["explain"]), &fake);
    assert_ne!(output.code, 0);
}
