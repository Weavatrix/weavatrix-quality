//! Task 11: unknown ids fail; argv is frozen; no user-injected executable.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
#[cfg(windows)]
use std::time::Duration;

use wvq_runtime::{
    Executor, ExecutorId, ExecutorRegistry, PrepareRequest, RuntimeError, default_limits,
    discover_executor_targets,
};
#[cfg(windows)]
use wvq_runtime::{ExecutorSpec, ProcessLimits};

fn request(id: &str, extra: BTreeMap<String, String>) -> PrepareRequest {
    PrepareRequest {
        executor: ExecutorId::new(id).unwrap(),
        cwd: std::env::temp_dir(),
        filters: Vec::new(),
        extra,
        limits: default_limits(),
        cancel: Arc::new(AtomicBool::new(false)),
    }
}

#[test]
fn unknown_executor_id_fails() {
    let registry = ExecutorRegistry::production().unwrap();
    let err = registry
        .prepare(request("bash", BTreeMap::new()))
        .unwrap_err();
    assert!(matches!(err, RuntimeError::UnknownExecutor(id) if id == "bash"));
}

#[test]
fn registered_go_test_gets_frozen_typed_argv() {
    let registry = ExecutorRegistry::production().unwrap();
    let mut req = request("go-test", BTreeMap::new());
    req.filters = vec!["TestAdd".into()];
    let prepared = registry.prepare(req).unwrap();
    assert_eq!(prepared.program, "go");
    assert_eq!(
        prepared.args,
        [
            "test",
            "-json",
            "-coverprofile=.weavatrix-quality/go-cover.out",
            "./...",
            "-run",
            "TestAdd"
        ]
    );
    assert_eq!(
        registry
            .capabilities(&prepared.executor)
            .map(|caps| caps.cases),
        Some(true)
    );
}

#[test]
fn path_filters_share_one_runner_process_without_becoming_name_patterns() {
    let registry = ExecutorRegistry::production().unwrap();
    let paths = vec!["tests/alpha.test.ts".into(), "tests/beta.test.ts".into()];

    let mut vitest = request("vitest", BTreeMap::new());
    vitest.filters.clone_from(&paths);
    assert_eq!(
        registry.prepare(vitest).unwrap().args,
        [
            "exec",
            "--offline",
            "--yes=false",
            "--",
            "vitest",
            "run",
            "--reporter=junit",
            "--outputFile=.weavatrix-quality/junit.xml",
            "tests/alpha.test.ts",
            "tests/beta.test.ts"
        ]
    );

    let mut jest = request("jest", BTreeMap::new());
    jest.filters = paths;
    assert_eq!(
        registry.prepare(jest).unwrap().args,
        [
            "--runInBand",
            "--runTestsByPath",
            "tests/alpha.test.ts",
            "tests/beta.test.ts"
        ]
    );
}

#[test]
fn repository_discovery_returns_only_registered_ids() {
    let root = std::env::temp_dir().join(format!("wvq-discovery-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("web")).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
    std::fs::write(
        root.join("web/package.json"),
        r#"{"scripts":{"test":"vitest run"},"devDependencies":{"vitest":"1"}}"#,
    )
    .unwrap();

    let targets = discover_executor_targets(&root).unwrap();
    let ids: Vec<&str> = targets
        .iter()
        .map(|target| target.executor.as_str())
        .collect();
    assert_eq!(ids, ["cargo-test", "vitest"]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn complex_repository_test_script_stays_on_the_generic_npm_boundary() {
    let root = std::env::temp_dir().join(format!("wvq-discovery-complex-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("package.json"),
        r#"{"scripts":{"test":"node prepare.mjs && vitest run --coverage"},"devDependencies":{"vitest":"4"}}"#,
    )
    .unwrap();

    let targets = discover_executor_targets(&root).unwrap();
    assert_eq!(targets[0].executor.as_str(), "npm-test");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn mcp_command_field_cannot_select_an_executable() {
    let registry = ExecutorRegistry::production().unwrap();
    let mut extra = BTreeMap::new();
    extra.insert("command".into(), "rm -rf /".into());
    extra.insert("program".into(), "powershell.exe".into());
    let err = registry.prepare(request("vitest", extra)).unwrap_err();
    assert!(matches!(err, RuntimeError::InvalidArg(msg) if msg.contains("executable")));
}

#[test]
fn filter_shell_metacharacters_are_rejected() {
    let registry = ExecutorRegistry::production().unwrap();
    let mut req = request("vitest", BTreeMap::new());
    req.filters = vec!["ok | calc.exe".into()];
    assert!(matches!(
        registry.prepare(req),
        Err(RuntimeError::InvalidArg(_))
    ));
}

#[cfg(windows)]
#[test]
fn deadline_output_and_cancel_limits_are_enforced() {
    let mut registry = ExecutorRegistry::new();
    registry
        .register(ExecutorSpec {
            id: ExecutorId::new("wvq-echo").unwrap(),
            program: "cmd.exe".into(),
            prefix: vec!["/d".into(), "/c".into(), "echo wvq-ok".into()],
            filter_flag: None,
            capabilities: wvq_runtime::ExecutorCapabilities {
                cases: false,
                coverage: false,
            },
        })
        .unwrap();
    registry
        .register(ExecutorSpec {
            id: ExecutorId::new("wvq-wait").unwrap(),
            program: "cmd.exe".into(),
            prefix: vec!["/d".into(), "/c".into(), "ping -n 20 127.0.0.1 >nul".into()],
            filter_flag: None,
            capabilities: wvq_runtime::ExecutorCapabilities {
                cases: false,
                coverage: false,
            },
        })
        .unwrap();

    let echo = registry
        .prepare(request("wvq-echo", BTreeMap::new()))
        .unwrap();
    let ok = registry.execute(&echo).unwrap();
    let text = String::from_utf8_lossy(&ok.stdout);
    assert!(text.contains("wvq-ok"), "{text}");

    let mut tiny = request("wvq-echo", BTreeMap::new());
    tiny.limits = ProcessLimits {
        deadline: Duration::from_secs(5),
        max_output_bytes: 1,
    };
    let prepared = registry.prepare(tiny).unwrap();
    assert!(matches!(
        registry.execute(&prepared),
        Err(RuntimeError::OutputLimit { max: 1 })
    ));

    let mut wait = request("wvq-wait", BTreeMap::new());
    wait.limits = ProcessLimits {
        deadline: Duration::from_millis(80),
        max_output_bytes: 64 * 1024,
    };
    let prepared = registry.prepare(wait).unwrap();
    assert!(matches!(
        registry.execute(&prepared),
        Err(RuntimeError::DeadlineExceeded)
    ));

    let cancelled = request("wvq-echo", BTreeMap::new());
    cancelled
        .cancel
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let prepared = registry.prepare(cancelled).unwrap();
    assert!(matches!(
        registry.execute(&prepared),
        Err(RuntimeError::Cancelled)
    ));
}
