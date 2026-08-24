//! Committed base/head product fixture across the public WVQ transports.
//!
//! The fixture is a real Git repository with a React/Vitest/Playwright
//! frontend, a Node backend, a Go service, and `OpenSpec`. Healthy relocation,
//! a surviving test that stops reaching its guard, and deletion of the sole
//! protector are all measured from clean commits. A loss must survive transport
//! boundaries.

use std::io::{Cursor, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mcport::serve_controlled_streams;
use qualityd::{HttpRequest, Studio};
use serde_json::{Value, json};
use wvq_command_bus::{LiveService, QualityService, VerifyCommand};
use wvq_mcp::{SharedDesk, quality_server, recovery_server, runtime_config};
use wvq_store::Store;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Default)]
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct PageServer {
    url: String,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl PageServer {
    fn start(body: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
        listener.set_nonblocking(true).expect("non-blocking server");
        let url = format!("http://{}", listener.local_addr().expect("fixture address"));
        let body = Arc::new(body.to_owned());
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            while !server_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let body = Arc::clone(&body);
                        thread::spawn(move || respond(stream, &body));
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            url,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for PageServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn respond(mut stream: TcpStream, body: &str) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut request = [0_u8; 4096];
    let _ = stream.read(&mut request);
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}

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

struct ProductFixture {
    repo: TempRepo,
    base: String,
    head: String,
    _base_server: PageServer,
    _head_server: PageServer,
}

#[derive(Clone, Copy)]
enum HeadScenario {
    HealthyRefactor,
    PhantomProtector,
    DeletedProtector,
    ApprovedExpectationReplacement,
    MissingSpecChangedSymbol,
}

fn workspace() -> PathBuf {
    let canonical = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    if cfg!(windows) {
        let display = canonical.to_string_lossy();
        if let Some(ordinary) = display.strip_prefix(r"\\?\") {
            return PathBuf::from(ordinary);
        }
    }
    canonical
}

fn unique_repo() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "wvq-product-monorepo-{}-{nanos}-{sequence}",
        std::process::id()
    ))
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = ProcessCommand::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git output")
        .trim()
        .to_owned()
}

fn commit(root: &Path, message: &str) -> String {
    git(root, &["add", "-A"]);
    git(
        root,
        &[
            "-c",
            "user.name=WVQ Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            message,
        ],
    );
    git(root, &["rev-parse", "HEAD"])
}

fn write(root: &Path, relative: &str, contents: impl AsRef<[u8]>) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("fixture directory");
    }
    std::fs::write(path, contents).expect("fixture file");
}

fn file_url(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        format!("file:///{normalized}")
    } else {
        format!("file://{normalized}")
    }
}

fn link_node_modules(root: &Path) {
    let source = workspace().join("js/playwright-runner/node_modules");
    let target = root.join("node_modules");
    #[cfg(windows)]
    {
        let output = ProcessCommand::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "New-Item -ItemType Junction -Path $env:WVQ_FIXTURE_TARGET -Target $env:WVQ_FIXTURE_SOURCE | Out-Null",
            ])
            .env("WVQ_FIXTURE_TARGET", &target)
            .env("WVQ_FIXTURE_SOURCE", &source)
            .output()
            .expect("PowerShell starts");
        assert!(
            output.status.success(),
            "create node_modules junction: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&source, &target).expect("node_modules symlink");
    assert!(
        target.join("playwright/package.json").is_file(),
        "fixture must reuse the workspace Playwright installation; source={} source_exists={} target_exists={} target={:?}",
        source.display(),
        source.join("playwright/package.json").is_file(),
        target.exists(),
        std::fs::read_link(&target)
    );
}

fn frontend_package() -> String {
    let vitest = workspace()
        .join("js/playwright-runner/node_modules/vitest/vitest.mjs")
        .to_string_lossy()
        .replace('\\', "/");
    serde_json::to_string_pretty(&json!({
        "name": "fixture-frontend",
        "private": true,
        "type": "module",
        "scripts": {
            "test": format!(
                "node \"{vitest}\" run --coverage --coverage.provider=v8 --coverage.reporter=lcov --reporter=junit --outputFile=.weavatrix-quality/junit.xml"
            )
        },
        "dependencies": {"react": "19.2.8"},
        "devDependencies": {
            "@vitest/coverage-v8": "4.1.11",
            "vitest": "4.1.11"
        }
    }))
    .expect("frontend package")
}

fn frontend_source(regression: bool) -> String {
    let react = file_url(&workspace().join("js/playwright-runner/node_modules/react/index.js"));
    let guard = if regression {
        "export function canDelete(_role) { return true; }"
    } else {
        "export function canDelete(role) { return role !== 'viewer'; }"
    };
    format!(
        "import React from '{react}';\n\
         export function viewerLabel() {{ return 'Viewer'; }}\n\
         {guard}\n\
         export function DeleteWidgetButton({{ role }}) {{\n  if (!canDelete(role)) return React.createElement('span', {{ 'data-testid': 'viewer-label' }}, viewerLabel());\n  return React.createElement('button', {{ 'data-testid': 'delete-widget' }}, 'Delete');\n}}\n"
    )
}

fn frontend_test(regression: bool) -> &'static str {
    if regression {
        "import { viewerLabel } from '../src/DeleteWidgetButton.mjs';\n\
         test('viewer cannot delete a widget', () => {\n  expect(viewerLabel()).toBe('Viewer');\n});\n"
    } else {
        "import { canDelete } from '../src/DeleteWidgetButton.mjs';\n\
         test('viewer cannot delete a widget', () => {\n  expect(canDelete('viewer')).toBe(false);\n});\n"
    }
}

fn quality_config(
    base_url: &str,
    binding_path: &str,
    binding_case: &str,
    binding_obligation: &str,
) -> String {
    format!(
        "quality_policy_v: 1\n\n\
         test_bindings:\n  - path: {binding_path}\n    runner: go-test\n    suite: fixture.local/product/service\n    case: {binding_case}\n    obligations: [{binding_obligation}]\n    cost: 10\n    flake_penalty: 0\n\n\
         browser:\n  base_url: {base_url}\n  engine: chromium\n  headless: true\n  timeout_ms: 120000\n  module_root: node_modules/playwright\n  programs:\n    - .weavatrix-quality/programs/viewer.json\n"
    )
}

#[allow(clippy::too_many_lines)]
fn product_fixture(scenario: HeadScenario) -> ProductFixture {
    let base_server =
        PageServer::start("<!doctype html><span data-testid=\"viewer-label\">Viewer</span>");
    let head_server = PageServer::start(
        "<!doctype html><span data-testid=\"viewer-label\">Viewer</span><button data-testid=\"delete-widget\">Delete</button>",
    );
    let root = unique_repo();

    write(
        &root,
        ".gitignore",
        "node_modules\nfrontend/coverage/\nfrontend/.weavatrix-quality/\n\
         backend/.weavatrix-quality/\nservice/.weavatrix-quality/\n\
         .weavatrix-quality/*.db*\n.weavatrix-quality/cas/\n.weavatrix-quality/objects/\n\
         .weavatrix-quality/runtime/\n.weavatrix-quality/browser-evidence/\n",
    );
    write(&root, "frontend/package.json", frontend_package());
    write(
        &root,
        "frontend/vitest.config.mjs",
        "export default { test: { globals: true, coverage: { enabled: true, provider: 'v8', reporter: ['lcov'], reportsDirectory: 'coverage' } } };\n",
    );
    write(
        &root,
        "frontend/src/DeleteWidgetButton.mjs",
        frontend_source(false),
    );
    write(
        &root,
        "frontend/tests/delete-widget.test.mjs",
        frontend_test(false),
    );
    write(
        &root,
        "backend/package.json",
        r#"{"name":"fixture-backend","private":true,"type":"module","scripts":{"test":"node --test"}}"#,
    );
    write(
        &root,
        "backend/permission-client.mjs",
        "export const permissionRoute = '/widgets/:id';\n",
    );
    write(
        &root,
        "backend/permission-client.test.mjs",
        "import test from 'node:test';\nimport assert from 'node:assert/strict';\nimport { permissionRoute } from './permission-client.mjs';\ntest('permission route stays stable', () => assert.equal(permissionRoute, '/widgets/:id'));\n",
    );
    write(
        &root,
        "service/go.mod",
        "module fixture.local/product/service\n\ngo 1.22\n",
    );
    write(
        &root,
        "service/permission.go",
        "package service\n\nfunc ViewerLabel() string { return \"Viewer\" }\n\nfunc CanDelete(role string) bool { return role != \"viewer\" }\n",
    );
    write(
        &root,
        "service/permission_test.go",
        "package service\n\nimport \"testing\"\n\nfunc TestViewerCannotDelete(t *testing.T) {\n\tif CanDelete(\"viewer\") { t.Fatal(\"viewer must be denied\") }\n\tif ViewerLabel() != \"Viewer\" { t.Fatal(\"label changed\") }\n}\n",
    );
    write(
        &root,
        "openspec/changes/viewer-delete/specs/widgets/spec.md",
        "# Widget permissions\n\n## ADDED Requirements\n\n### Requirement: Viewer permissions\nThe system SHALL NOT allow a viewer to delete a widget.\n\n#### Scenario: Viewer opens a widget\n- GIVEN a viewer\n- WHEN the widget is opened\n- THEN the viewer remains identified and delete is denied\n",
    );
    write(
        &root,
        "openspec/changes/viewer-delete/quality.yaml",
        "quality_contract_v: 1\nchange: viewer-delete\n\nrisk:\n  default: high\n\nrequirements:\n  - capability: widgets\n    requirement: viewer-permissions\n    scenarios:\n      - scenario: viewer-opens-a-widget\n        obligations:\n          - id: viewer-deny\n            kind: invariant\n          - id: viewer-label-visible\n            kind: behavioral\n            expected:\n              kind: visible\n              target:\n                test_id: viewer-label\n        evidence:\n          required: []\n          on_failure: [screenshot]\n\nai:\n  planning_tokens: 100\n  runtime_tokens: 0\n",
    );
    write(
        &root,
        ".weavatrix-quality/programs/viewer.json",
        r#"{
  "schema_v": 1,
  "id": "viewer-widget",
  "source": "authored",
  "obligations": ["viewer-label-visible"],
  "steps": [
    {"action": "navigate", "route": "/"},
    {"action": "assert", "obligation": "viewer-label-visible"}
  ],
  "evidence_policy": {
    "screenshot": "on_failure",
    "trace": "never",
    "network": "always",
    "console": "always",
    "storage": "never"
  }
}"#,
    );
    write(
        &root,
        ".weavatrix-quality/config.yaml",
        quality_config(
            &base_server.url,
            "service/permission_test.go",
            "TestViewerCannotDelete",
            "viewer-deny",
        ),
    );

    git(&root, &["init", "-q"]);
    let base = commit(&root, "A: viewer denial is protected");

    let (message, binding_path, binding_case, binding_obligation) = match scenario {
        HeadScenario::HealthyRefactor => {
            std::fs::rename(
                root.join("service/permission.go"),
                root.join("service/authorization.go"),
            )
            .expect("move implementation");
            write(
                &root,
                "service/authorization.go",
                "package service\n\nfunc ViewerLabel() string { return \"Viewer\" }\n\n// Permission policy moved without changing behavior.\nfunc CanDelete(role string) bool { return role != \"viewer\" }\n",
            );
            std::fs::rename(
                root.join("service/permission_test.go"),
                root.join("service/authorization_test.go"),
            )
            .expect("move protector");
            (
                "B1: implementation and protector move together",
                "service/authorization_test.go",
                "TestViewerCannotDelete",
                "viewer-deny",
            )
        }
        HeadScenario::PhantomProtector => {
            write(
                &root,
                "frontend/src/DeleteWidgetButton.mjs",
                frontend_source(true),
            );
            write(
                &root,
                "frontend/tests/delete-widget.test.mjs",
                frontend_test(true),
            );
            write(
                &root,
                "service/permission.go",
                "package service\n\nfunc ViewerLabel() string { return \"Viewer\" }\n\nfunc CanDelete(_role string) bool { return true }\n",
            );
            write(
                &root,
                "service/permission_test.go",
                "package service\n\nimport \"testing\"\n\nfunc TestViewerCannotDelete(t *testing.T) {\n\tif 2 + 2 != 4 { t.Fatal(\"arithmetic changed\") }\n}\n",
            );
            (
                "B2: guard disappears while tests stay green",
                "service/permission_test.go",
                "TestViewerCannotDelete",
                "viewer-deny",
            )
        }
        HeadScenario::DeletedProtector => {
            std::fs::remove_file(root.join("service/permission_test.go"))
                .expect("remove sole protector");
            write(
                &root,
                "service/label_test.go",
                "package service\n\nimport \"testing\"\n\nfunc TestViewerLabel(t *testing.T) {\n\tif ViewerLabel() != \"Viewer\" { t.Fatal(\"label changed\") }\n}\n",
            );
            (
                "B3: delete the only viewer-denial test",
                "service/permission_test.go",
                "TestViewerCannotDelete",
                "viewer-deny",
            )
        }
        HeadScenario::ApprovedExpectationReplacement => {
            write(
                &root,
                "frontend/src/DeleteWidgetButton.mjs",
                frontend_source(true),
            );
            write(
                &root,
                "frontend/tests/delete-widget.test.mjs",
                "import { canDelete, DeleteWidgetButton } from '../src/DeleteWidgetButton.mjs';\n\
                 test('viewer can delete a widget', () => {\n  expect(canDelete('viewer')).toBe(true);\n  expect(DeleteWidgetButton({ role: 'viewer' }).type).toBe('button');\n});\n",
            );
            write(
                &root,
                "service/permission.go",
                "package service\n\nfunc ViewerLabel() string { return \"Viewer\" }\n\nfunc CanDelete(_role string) bool { return true }\n",
            );
            write(
                &root,
                "service/permission_test.go",
                "package service\n\nimport \"testing\"\n\nfunc TestViewerCanDelete(t *testing.T) {\n\tif !CanDelete(\"viewer\") { t.Fatal(\"viewer must be allowed\") }\n}\n",
            );
            write(
                &root,
                "openspec/changes/viewer-delete/specs/widgets/spec.md",
                "# Widget permissions\n\n## ADDED Requirements\n\n### Requirement: Viewer permissions\nThe system SHALL allow a viewer to delete a widget.\n\n#### Scenario: Viewer opens a widget\n- GIVEN a viewer\n- WHEN the widget is opened\n- THEN the delete action is available\n",
            );
            write(
                &root,
                "openspec/changes/viewer-delete/quality.yaml",
                "quality_contract_v: 1\nchange: viewer-delete\n\nrisk:\n  default: high\n\nrequirements:\n  - capability: widgets\n    requirement: viewer-permissions\n    scenarios:\n      - scenario: viewer-opens-a-widget\n        obligations:\n          - id: viewer-allow\n            kind: invariant\n          - id: viewer-delete-visible\n            kind: behavioral\n            expected:\n              kind: visible\n              target:\n                test_id: delete-widget\n        evidence:\n          required: []\n          on_failure: [screenshot]\n\nai:\n  planning_tokens: 100\n  runtime_tokens: 0\n",
            );
            write(
                &root,
                ".weavatrix-quality/programs/viewer.json",
                r#"{
  "schema_v": 1,
  "id": "viewer-widget",
  "source": "authored",
  "obligations": ["viewer-delete-visible"],
  "steps": [
    {"action": "navigate", "route": "/"},
    {"action": "assert", "obligation": "viewer-delete-visible"}
  ],
  "evidence_policy": {
    "screenshot": "on_failure",
    "trace": "never",
    "network": "always",
    "console": "always",
    "storage": "never"
  }
}"#,
            );
            (
                "B4: replace denial with an intended allow rule",
                "service/permission_test.go",
                "TestViewerCanDelete",
                "viewer-allow",
            )
        }
        HeadScenario::MissingSpecChangedSymbol => {
            write(
                &root,
                "service/permission.go",
                "package service\n\nfunc ViewerLabel() string { return \"Viewer\" }\n\nfunc CanDelete(_role string) bool { return true }\n",
            );
            write(
                &root,
                "service/permission_test.go",
                "package service\n\nimport \"testing\"\n\nfunc TestViewerCanDelete(t *testing.T) {\n\tif !CanDelete(\"viewer\") { t.Fatal(\"viewer must be allowed\") }\n}\n",
            );
            (
                "B5: code and protector change without declared intent",
                "service/permission_test.go",
                "TestViewerCanDelete",
                "viewer-deny",
            )
        }
    };
    write(
        &root,
        ".weavatrix-quality/config.yaml",
        quality_config(
            &head_server.url,
            binding_path,
            binding_case,
            binding_obligation,
        ),
    );
    let head = commit(&root, message);
    link_node_modules(&root);
    let status = git(&root, &["status", "--porcelain"]);
    assert!(status.is_empty(), "fixture worktree is dirty:\n{status}");

    ProductFixture {
        repo: TempRepo(root),
        base,
        head,
        _base_server: base_server,
        _head_server: head_server,
    }
}

fn protocol_verify(service: &Arc<dyn QualityService>) -> String {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "quality_verify",
            "arguments": {"change": "viewer-delete"}
        }
    });
    let input = Cursor::new(format!("{request}\n").into_bytes());
    let writer = SharedWriter::default();
    let captured = writer.clone();
    serve_controlled_streams(
        Arc::new(quality_server(service)),
        input,
        writer,
        runtime_config(),
    )
    .expect("MCP call");
    String::from_utf8(captured.0.lock().unwrap().clone()).expect("MCP UTF-8")
}

fn protocol_recovery_review(desk: &SharedDesk) -> String {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "quality_spec_review",
            "arguments": {}
        }
    });
    let input = Cursor::new(format!("{request}\n").into_bytes());
    let writer = SharedWriter::default();
    let captured = writer.clone();
    serve_controlled_streams(
        Arc::new(recovery_server(desk)),
        input,
        writer,
        runtime_config(),
    )
    .expect("recovery MCP call");
    String::from_utf8(captured.0.lock().unwrap().clone()).expect("MCP UTF-8")
}

#[test]
fn committed_monorepo_preserves_protection_through_a_healthy_refactor() {
    let fixture = product_fixture(HeadScenario::HealthyRefactor);
    let live = LiveService::new(&fixture.repo.0);
    let view = live
        .protection_view("viewer-delete", &fixture.base, &fixture.head)
        .expect("measure committed healthy refactor");
    assert!(
        view.deltas.iter().all(|delta| !matches!(
            delta.state,
            wvq_proof::ProtectionDeltaState::Lost
                | wvq_proof::ProtectionDeltaState::Degraded
                | wvq_proof::ProtectionDeltaState::NewUnprotected
        )),
        "a pure move must preserve the safety net: {:?}",
        view.deltas
    );
    assert!(!view.report().blocking, "{:?}", view.findings);
    let verified = live
        .verify(&VerifyCommand {
            change: "viewer-delete".into(),
        })
        .expect("healthy refactor verdict");
    assert!(
        matches!(verified.state.as_str(), "PASS" | "PASS_WITH_WARNINGS"),
        "healthy refactor must pass: {:?}",
        verified.quality.blocking_reasons
    );
    assert_eq!(verified.verdict, "PROVEN");
}

#[test]
#[allow(clippy::too_many_lines)]
fn committed_monorepo_blocks_the_same_measured_loss_in_cli_mcp_and_qualityd() {
    let fixture = product_fixture(HeadScenario::PhantomProtector);
    let live = LiveService::new(&fixture.repo.0);
    let view = live
        .protection_view("viewer-delete", &fixture.base, &fixture.head)
        .expect("measure committed base/head protection");
    assert!(
        view.deltas
            .iter()
            .any(|delta| delta.state.as_str() == "lost"),
        "the denial guard must lose its measured protection: {:?}",
        view.deltas
    );
    let exact = "service/permission_test.go#TestViewerCannotDelete";
    assert!(
        view.lineage
            .iter()
            .any(|lineage| lineage.test == exact && lineage.phantom),
        "the still-green exact case must be a phantom protector: {:?}",
        view.lineage
    );
    assert!(
        view.findings
            .iter()
            .any(|finding| finding.check.as_str() == "WVQ-PROTECT-002"),
        "live protection must report the surviving test that lost its flow: {:?}",
        view.findings
    );

    let root = fixture.repo.0.to_str().expect("UTF-8 fixture path");
    let cli = wvq_cli::run(&[
        "--repo".into(),
        root.into(),
        "verify".into(),
        "--change".into(),
        "viewer-delete".into(),
    ]);
    assert_eq!(cli.code, 2, "{}", cli.stderr);
    let cli_json: Value = serde_json::from_str(&cli.stdout).expect("CLI JSON");
    let cli_body = &cli_json["body"];
    assert_eq!(cli_body["verdict"], "PROVEN");
    assert_eq!(cli_body["state"], "BLOCKED");
    assert_eq!(cli_body["quality"]["protection"]["state"], "blocking");

    let mcp_service: Arc<dyn QualityService> = Arc::new(LiveService::new(&fixture.repo.0));
    let mcp = protocol_verify(&mcp_service);
    assert!(mcp.contains("\"verdict\":\"PROVEN\""), "{mcp}");
    assert!(mcp.contains("\"state\":\"BLOCKED\""), "{mcp}");
    assert!(mcp.contains("WVQ-PROTECT-002"), "{mcp}");

    let studio_service: Arc<dyn QualityService> = Arc::new(LiveService::new(&fixture.repo.0));
    let studio = Studio::new(
        studio_service,
        Store::open(&fixture.repo.0).expect("fixture store"),
    );
    let response = studio.handle(&HttpRequest {
        method: "GET".into(),
        path: "/api/v1/changes/viewer-delete/summary".into(),
        body: String::new(),
    });
    assert_eq!(response.status, 200, "{}", response.body);
    let body: Value = serde_json::from_str(&response.body).expect("Studio JSON");
    assert_eq!(body["verdict"], "PROVEN");
    assert_eq!(body["state"], "BLOCKED");
    assert_eq!(body["blocking"], true);
    assert!(body["axes"].as_array().is_some_and(|axes| {
        axes.iter()
            .any(|axis| axis["axis"] == "protection" && axis["state"] == "blocking")
    }));

    let direct = LiveService::new(&fixture.repo.0)
        .verify(&VerifyCommand {
            change: "viewer-delete".into(),
        })
        .expect("direct verdict");
    assert_eq!(direct.quality.ai.runtime_tokens, 0);
}

#[test]
fn committed_monorepo_reports_a_deleted_sole_protector_as_protect_003() {
    let fixture = product_fixture(HeadScenario::DeletedProtector);
    let live = LiveService::new(&fixture.repo.0);
    let view = live
        .protection_view("viewer-delete", &fixture.base, &fixture.head)
        .expect("measure committed deleted-protector change");
    let exact = "service/permission_test.go#TestViewerCannotDelete";
    assert!(
        view.lineage
            .iter()
            .any(|lineage| lineage.test == exact && lineage.state == "removed"),
        "the exact base protector must be absent on head: {:?}",
        view.lineage
    );
    assert!(
        view.findings
            .iter()
            .any(|finding| finding.check.as_str() == "WVQ-PROTECT-003"),
        "a deleted sole proof path must be named: {:?}",
        view.findings
    );
    let verified = live
        .verify(&VerifyCommand {
            change: "viewer-delete".into(),
        })
        .expect("deleted-protector verdict");
    assert_eq!(verified.state, "BLOCKED");
    assert!(
        verified
            .quality
            .protection
            .blocking_findings
            .iter()
            .any(|finding| finding.check.as_str() == "WVQ-PROTECT-003")
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn committed_monorepo_replaces_an_expectation_only_after_one_exact_approval() {
    let fixture = product_fixture(HeadScenario::ApprovedExpectationReplacement);
    let live = LiveService::new(&fixture.repo.0);
    let pending = live
        .protection_view("viewer-delete", &fixture.base, &fixture.head)
        .expect("measure committed expectation replacement");
    let review = pending
        .oracle_replacement
        .clone()
        .expect("a changed OracleSeal creates one review packet");
    assert_eq!(review.base_revision, fixture.base);
    assert_eq!(review.head_revision, fixture.head);
    assert_eq!(review.merge_base, fixture.base);
    assert_ne!(review.base_seal_digest, review.head_seal_digest);
    assert_eq!(
        review.obligation_replacements,
        [
            ("viewer-deny".into(), "viewer-allow".into()),
            (
                "viewer-label-visible".into(),
                "viewer-delete-visible".into()
            ),
        ]
    );
    assert!(!review.approved);
    assert!(pending.report().blocking);

    let studio_service: Arc<dyn QualityService> = Arc::new(LiveService::new(&fixture.repo.0));
    let studio = Studio::new(
        studio_service,
        Store::open(&fixture.repo.0).expect("fixture store"),
    );

    // A broad or stale approval cannot silently bless a different proposal.
    let stale = studio.handle(&HttpRequest {
        method: "POST".into(),
        path: "/api/v1/human-decisions".into(),
        body: json!({
            "id": "hd-b4-stale",
            "reviewer": "quality-agent",
            "role": "product",
            "subject": review.subject,
            "artifact_digest": "0000000000000000000000000000000000000000000000000000000000000000",
            "decision": "accept_as_intended",
            "comment": "proposal prepared automatically; stale digest must not match",
            "decided_at": "2026-08-24T12:00:00Z"
        })
        .to_string(),
    });
    assert_eq!(stale.status, 201, "{}", stale.body);
    let still_pending = live
        .verify(&VerifyCommand {
            change: "viewer-delete".into(),
        })
        .expect("stale approval stays pending");
    assert_eq!(still_pending.state, "BLOCKED");

    let developer = studio.handle(&HttpRequest {
        method: "POST".into(),
        path: "/api/v1/human-decisions".into(),
        body: json!({
            "id": "hd-b4-developer",
            "reviewer": "implementation-agent",
            "role": "developer",
            "subject": review.subject,
            "artifact_digest": review.artifact_digest,
            "decision": "accept_as_intended",
            "comment": "implementation evidence cannot establish business intent",
            "decided_at": "2026-08-24T12:00:30Z"
        })
        .to_string(),
    });
    assert_eq!(developer.status, 201, "{}", developer.body);
    assert_eq!(
        live.verify(&VerifyCommand {
            change: "viewer-delete".into(),
        })
        .expect("developer approval cannot seal intent")
        .state,
        "BLOCKED"
    );

    // All analysis is automatic. The only human action is accepting this one
    // exact, immutable replacement packet.
    let accepted = studio.handle(&HttpRequest {
        method: "POST".into(),
        path: "/api/v1/human-decisions".into(),
        body: json!({
            "id": "hd-b4-exact",
            "reviewer": "intent-owner",
            "role": "product",
            "subject": review.subject,
            "artifact_digest": review.artifact_digest,
            "decision": "accept_as_intended",
            "comment": "the replacement shown by WVQ is the intended business behavior",
            "decided_at": "2026-08-24T12:01:00Z"
        })
        .to_string(),
    });
    assert_eq!(accepted.status, 201, "{}", accepted.body);

    let direct = live
        .verify(&VerifyCommand {
            change: "viewer-delete".into(),
        })
        .expect("exact approval unlocks the composite verdict");
    assert_eq!(direct.verdict, "PROVEN");
    assert!(
        matches!(direct.state.as_str(), "PASS" | "PASS_WITH_WARNINGS"),
        "approved expectation replacement must pass: {:?}",
        direct.quality.blocking_reasons
    );
    assert!(direct.quality.protection.summary.replaced >= 1);
    assert_eq!(direct.quality.ai.runtime_tokens, 0);

    let root = fixture.repo.0.to_str().expect("UTF-8 fixture path");
    let cli = wvq_cli::run(&[
        "--repo".into(),
        root.into(),
        "verify".into(),
        "--change".into(),
        "viewer-delete".into(),
    ]);
    assert_eq!(cli.code, 0, "{}", cli.stderr);
    let cli_json: Value = serde_json::from_str(&cli.stdout).expect("CLI JSON");
    assert_eq!(cli_json["body"]["verdict"], "PROVEN");
    assert!(matches!(
        cli_json["body"]["state"].as_str(),
        Some("PASS" | "PASS_WITH_WARNINGS")
    ));
    assert!(
        cli_json["body"]["quality"]["protection"]["summary"]["replaced"]
            .as_u64()
            .is_some_and(|count| count >= 1)
    );

    let mcp_service: Arc<dyn QualityService> = Arc::new(LiveService::new(&fixture.repo.0));
    let mcp = protocol_verify(&mcp_service);
    assert!(mcp.contains("\"verdict\":\"PROVEN\""), "{mcp}");
    assert!(
        mcp.contains("\"state\":\"PASS\"") || mcp.contains("\"state\":\"PASS_WITH_WARNINGS\""),
        "{mcp}"
    );
    assert!(mcp.contains("\"replaced\":"), "{mcp}");

    let summary = studio.handle(&HttpRequest {
        method: "GET".into(),
        path: "/api/v1/changes/viewer-delete/summary".into(),
        body: String::new(),
    });
    assert_eq!(summary.status, 200, "{}", summary.body);
    let summary: Value = serde_json::from_str(&summary.body).expect("Studio summary JSON");
    assert_eq!(summary["verdict"], "PROVEN");
    assert_eq!(summary["blocking"], false);
    assert!(matches!(
        summary["state"].as_str(),
        Some("PASS" | "PASS_WITH_WARNINGS")
    ));
}

#[test]
fn committed_b5_routes_one_changed_public_symbol_to_review_without_auto_sealing() {
    let fixture = product_fixture(HeadScenario::MissingSpecChangedSymbol);
    let live = LiveService::new(&fixture.repo.0);
    let mut desk = live
        .recovery_desk("viewer-delete", &fixture.base, &fixture.head)
        .expect("build committed B5 recovery packet");
    let packet = desk.packet().expect("B5 packet");
    assert_eq!(packet.base_revision, fixture.base);
    assert_eq!(packet.head_revision, fixture.head);
    assert_eq!(
        packet.code_delta_summary.public_symbols.len(),
        1,
        "only the business function should survive public-symbol filtering: {:?}",
        packet.code_delta_summary.public_symbols
    );
    assert!(
        packet.code_delta_summary.public_symbols[0].contains("CanDelete"),
        "the graph must name the changed business symbol: {:?}",
        packet.code_delta_summary.public_symbols
    );
    assert_eq!(packet.tests_delta.changed, ["service/permission_test.go"]);
    assert!(
        !packet.neighboring_requirements.is_empty(),
        "nested OpenSpec remains visible as context even though it did not change"
    );

    let review = desk.review();
    assert_eq!(review.candidates.len(), 1);
    let candidate = &review.candidates[0];
    assert_eq!(candidate.state, "QA_REVIEW");
    assert!(candidate.requires_product_approval);
    assert!(
        candidate
            .findings
            .iter()
            .any(|finding| finding.contains("weak_oracle_independence"))
    );
    let candidate_id = candidate.id.clone();
    assert!(
        desk.seal(&candidate_id)
            .unwrap_err()
            .to_string()
            .contains("cannot seal without QA verification")
    );

    let root = fixture.repo.0.to_str().expect("UTF-8 fixture path");
    let cli = wvq_cli::run(&[
        "--repo".into(),
        root.into(),
        "recover".into(),
        "--change".into(),
        "viewer-delete".into(),
        "--base".into(),
        fixture.base.clone(),
        "--head".into(),
        fixture.head.clone(),
    ]);
    assert_eq!(cli.code, 0, "{}", cli.stderr);
    let cli_json: Value = serde_json::from_str(&cli.stdout).expect("CLI recovery JSON");
    assert_eq!(cli_json["command"], "recovery");
    assert_eq!(
        cli_json["body"]["review"]["candidates"][0]["state"],
        "QA_REVIEW"
    );
    assert_eq!(cli_json["body"]["runtime_llm_tokens"], 0);

    let shared = Arc::new(Mutex::new(desk));
    let mcp = protocol_recovery_review(&shared);
    assert!(mcp.contains("QA_REVIEW"), "{mcp}");
    assert!(mcp.contains("weak_oracle_independence"), "{mcp}");
    assert!(mcp.contains("CanDelete"), "{mcp}");

    let studio_service: Arc<dyn QualityService> = Arc::new(LiveService::new(&fixture.repo.0));
    let studio = Studio::new(
        studio_service,
        Store::open(&fixture.repo.0).expect("fixture store"),
    )
    .with_recovery(shared);
    let response = studio.handle(&HttpRequest {
        method: "GET".into(),
        path: "/api/v1/recovery/review".into(),
        body: String::new(),
    });
    assert_eq!(response.status, 200, "{}", response.body);
    let body: Value = serde_json::from_str(&response.body).expect("Studio recovery JSON");
    assert_eq!(body["candidates"][0]["state"], "QA_REVIEW");
    assert_eq!(body["candidates"].as_array().map(Vec::len), Some(1));
}
