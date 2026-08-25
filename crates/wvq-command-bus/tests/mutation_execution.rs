use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::{SystemTime, UNIX_EPOCH};

use wvq_command_bus::{EvidenceCommand, LiveService, QualityService, RunCommand, VerifyCommand};

struct TempRepo(PathBuf);

impl Drop for TempRepo {
    fn drop(&mut self) {
        let node_modules = self.0.join("node_modules");
        let links_outside = node_modules
            .canonicalize()
            .ok()
            .zip(self.0.canonicalize().ok())
            .is_some_and(|(target, root)| !target.starts_with(root));
        if links_outside {
            let _ = std::fs::remove_dir(&node_modules);
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(root: &Path, args: &[&str]) {
    let output = ProcessCommand::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn link_node_modules(root: &Path) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("js/playwright-runner/node_modules");
    let target = root.join("node_modules");
    #[cfg(windows)]
    {
        let output = ProcessCommand::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "New-Item -ItemType Junction -Path $env:WVQ_TEST_LINK_TARGET -Target $env:WVQ_TEST_LINK_SOURCE | Out-Null",
            ])
            .env("WVQ_TEST_LINK_TARGET", &target)
            .env("WVQ_TEST_LINK_SOURCE", &source)
            .output()
            .unwrap();
        assert!(output.status.success());
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(source, target).unwrap();
}

fn mutation_repo(kills_boundary: bool) -> TempRepo {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("wvq-mutation-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(root.join("limit")).unwrap();
    std::fs::create_dir_all(root.join("openspec/changes/limit-change/specs/limits")).unwrap();
    std::fs::create_dir_all(root.join(".weavatrix-quality")).unwrap();
    std::fs::write(
        root.join("go.mod"),
        "module fixture.local/mutation\n\ngo 1.24\n",
    )
    .unwrap();
    std::fs::write(
        root.join("limit/limit.go"),
        "package limit\n\nfunc Allowed(value int) bool {\n\treturn value > 5\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("limit/limit_test.go"),
        "package limit\n\nimport \"testing\"\n\nfunc TestAllowed(t *testing.T) {\n\tif !Allowed(6) { t.Fatal(\"six must be allowed\") }\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("openspec/changes/limit-change/specs/limits/spec.md"),
        "# Delta for Limits\n\n## ADDED Requirements\n\n### Requirement: Inclusive limit\nThe system SHALL accept values at and above the limit.\n\n#### Scenario: Allowed value\n- GIVEN a value above the limit\n- WHEN permission is evaluated\n- THEN it is allowed\n",
    )
    .unwrap();
    std::fs::write(
        root.join("openspec/changes/limit-change/quality.yaml"),
        "quality_contract_v: 1\nchange: limit-change\n\nrisk:\n  default: high\n\nrequirements:\n  - capability: limits\n    requirement: inclusive-limit\n    scenarios:\n      - scenario: allowed-value\n        obligations:\n          - id: limit-allowed\n            kind: invariant\n        evidence:\n          required: []\n          on_failure: []\n        mutation:\n          operators: [boundary_flip]\n",
    )
    .unwrap();
    std::fs::write(
        root.join(".weavatrix-quality/config.yaml"),
        "quality_policy_v: 1\n\ntest_bindings:\n  - path: limit/limit_test.go\n    runner: go-test\n    suite: fixture.local/mutation/limit\n    case: TestAllowed\n    obligations: [limit-allowed]\n    cost: 1\n    flake_penalty: 0\n",
    )
    .unwrap();
    std::fs::write(
        root.join(".gitignore"),
        ".weavatrix-quality/*.db*\n.weavatrix-quality/cas/\n.weavatrix-quality/runtime/\n.weavatrix-quality/go-cover.out\n",
    )
    .unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(
        &root,
        &[
            "-c",
            "user.name=WVQ Test",
            "-c",
            "user.email=wvq@example.invalid",
            "commit",
            "-qm",
            "base",
        ],
    );
    std::fs::write(
        root.join("limit/limit.go"),
        "package limit\n\nfunc Allowed(value int) bool {\n\treturn value >= 5\n}\n",
    )
    .unwrap();
    if kills_boundary {
        std::fs::write(
            root.join("limit/limit_test.go"),
            "package limit\n\nimport \"testing\"\n\nfunc TestAllowed(t *testing.T) {\n\tif !Allowed(5) { t.Fatal(\"the boundary must be allowed\") }\n}\n",
        )
        .unwrap();
    }
    TempRepo(root)
}

fn js_mutation_repo() -> TempRepo {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("wvq-js-mutation-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("openspec/changes/limit-change/specs/limits")).unwrap();
    std::fs::create_dir_all(root.join(".weavatrix-quality")).unwrap();
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"wvq-js-mutation","private":true,"type":"module","scripts":{"test":"vitest run"},"devDependencies":{"vitest":"4.1.11"}}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/limit.js"),
        "export function allowed(value) {\n  return value > 5\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("limit.test.js"),
        "import { test, expect } from 'vitest'\nimport { allowed } from './src/limit.js'\n\ntest('weak boundary', () => { expect(allowed(6)).toBe(true) })\ntest('unbound strong boundary', () => { expect(allowed(5)).toBe(true) })\n",
    )
    .unwrap();
    std::fs::write(
        root.join("openspec/changes/limit-change/specs/limits/spec.md"),
        "# Delta for Limits\n\n## ADDED Requirements\n\n### Requirement: Inclusive limit\nThe system SHALL accept values at and above the limit.\n\n#### Scenario: Allowed value\n- GIVEN a value above the limit\n- WHEN permission is evaluated\n- THEN it is allowed\n",
    )
    .unwrap();
    std::fs::write(
        root.join("openspec/changes/limit-change/quality.yaml"),
        "quality_contract_v: 1\nchange: limit-change\n\nrisk:\n  default: high\n\nrequirements:\n  - capability: limits\n    requirement: inclusive-limit\n    scenarios:\n      - scenario: allowed-value\n        obligations:\n          - id: limit-allowed\n            kind: invariant\n        evidence:\n          required: []\n          on_failure: []\n        mutation:\n          operators: [boundary_flip]\n",
    )
    .unwrap();
    std::fs::write(
        root.join(".weavatrix-quality/config.yaml"),
        "quality_policy_v: 1\n\ntest_bindings:\n  - path: limit.test.js\n    runner: vitest\n    case: weak boundary\n    obligations: [limit-allowed]\n    cost: 1\n    flake_penalty: 0\n",
    )
    .unwrap();
    std::fs::write(
        root.join(".gitignore"),
        "node_modules/\n.weavatrix-quality/*.db*\n.weavatrix-quality/cas/\n.weavatrix-quality/runtime/\n.weavatrix-quality/junit.xml\n",
    )
    .unwrap();
    link_node_modules(&root);
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(
        &root,
        &[
            "-c",
            "user.name=WVQ Test",
            "-c",
            "user.email=wvq@example.invalid",
            "commit",
            "-qm",
            "base",
        ],
    );
    std::fs::write(
        root.join("src/limit.js"),
        "export function allowed(value) {\n  return value >= 5\n}\n",
    )
    .unwrap();
    TempRepo(root)
}

#[test]
fn a_surviving_real_source_mutant_weakens_an_otherwise_green_proof() {
    let repo = mutation_repo(false);
    let original = std::fs::read_to_string(repo.0.join("limit/limit.go")).unwrap();
    let service = LiveService::new(&repo.0);
    let run = service
        .run(&RunCommand {
            change: "limit-change".into(),
            base: "HEAD".into(),
            head: "WORKTREE".into(),
            scope: "all".into(),
            evidence_policy: "standard".into(),
        })
        .unwrap();
    assert_eq!(run.outcome, "passed", "{run:#?}");
    assert_eq!(
        std::fs::read_to_string(repo.0.join("limit/limit.go")).unwrap(),
        original,
        "mutation execution must never edit the user's checkout"
    );

    let handle = run
        .artifact_handles
        .iter()
        .find(|handle| handle.contains("mutation-results"))
        .expect("normal run persists mutation evidence");
    let evidence = service
        .evidence(&EvidenceCommand {
            handle: handle.clone(),
        })
        .unwrap();
    let document: serde_json::Value =
        serde_json::from_str(evidence.inline_text.as_deref().unwrap()).unwrap();
    assert_eq!(document["state"], "measured", "{document:#}");
    assert_eq!(document["planned"], 1);
    assert_eq!(document["killed"], 0);
    assert_eq!(document["survived"], 1);
    assert_eq!(document["invalid"], 0);
    assert_eq!(document["results"][0]["operator"], "boundary_flip");
    assert_eq!(document["results"][0]["path"], "limit/limit.go");
    assert_eq!(document["results"][0]["line"], 4);
    assert_eq!(
        document["results"][0]["tests_run"][0],
        "go-test#TestAllowed"
    );

    let verified = service
        .verify(&VerifyCommand {
            observe_only: false,
            change: "limit-change".into(),
        })
        .unwrap();
    assert_eq!(verified.verdict, "PARTIAL", "{verified:#?}");
    assert_eq!(verified.state, "NOT_ENOUGH_EVIDENCE", "{verified:#?}");
    assert_eq!(verified.exit_code(), 1);
}

#[test]
fn an_exact_test_that_detects_the_real_mutant_keeps_the_proof_proven() {
    let repo = mutation_repo(true);
    let service = LiveService::new(&repo.0);
    let run = service
        .run(&RunCommand {
            change: "limit-change".into(),
            base: "HEAD".into(),
            head: "WORKTREE".into(),
            scope: "all".into(),
            evidence_policy: "standard".into(),
        })
        .unwrap();
    let handle = run
        .artifact_handles
        .iter()
        .find(|handle| handle.contains("mutation-results"))
        .unwrap();
    let evidence = service
        .evidence(&EvidenceCommand {
            handle: handle.clone(),
        })
        .unwrap();
    let document: serde_json::Value =
        serde_json::from_str(evidence.inline_text.as_deref().unwrap()).unwrap();
    assert_eq!(document["planned"], 1);
    assert_eq!(document["killed"], 1, "{document:#}");
    assert_eq!(document["survived"], 0);
    assert_eq!(document["invalid"], 0);

    let verified = service
        .verify(&VerifyCommand {
            observe_only: false,
            change: "limit-change".into(),
        })
        .unwrap();
    assert_eq!(verified.verdict, "PROVEN", "{verified:#?}");
}

#[test]
fn a_real_vitest_case_judges_a_changed_line_javascript_mutant() {
    let repo = js_mutation_repo();
    let service = LiveService::new(&repo.0);
    let run = service
        .run(&RunCommand {
            change: "limit-change".into(),
            base: "HEAD".into(),
            head: "WORKTREE".into(),
            scope: "all".into(),
            evidence_policy: "standard".into(),
        })
        .unwrap();
    assert_eq!(run.outcome, "passed", "{run:#?}");
    let handle = run
        .artifact_handles
        .iter()
        .find(|handle| handle.contains("mutation-results"))
        .unwrap();
    let evidence = service
        .evidence(&EvidenceCommand {
            handle: handle.clone(),
        })
        .unwrap();
    let document: serde_json::Value =
        serde_json::from_str(evidence.inline_text.as_deref().unwrap()).unwrap();
    assert_eq!(document["state"], "measured", "{document:#}");
    assert_eq!(document["planned"], 1);
    assert_eq!(document["survived"], 1);
    assert_eq!(document["invalid"], 0);
    assert_eq!(document["results"][0]["ecosystem"], "ts_js");
    assert_eq!(
        document["results"][0]["tests_run"][0],
        "vitest#weak boundary"
    );
}
