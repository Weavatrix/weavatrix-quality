//! Real Chromium base/head UI-integrity regressions.
//!
//! The scenario these tests exist for is the one a behavioural suite cannot
//! catch on its own: the sealed test still passes, and the change has quietly
//! made a control unusable. Everything here runs actual Chromium against two
//! actual revisions and goes through the product path — run, collect, detect,
//! ratchet, store, `quality_verify`, `quality_explain`.
//!
//! The base and head builds of the fixture app are served on their own ports,
//! which is what the versioned `browser.base_url` points at in a real project
//! with per-revision preview deployments. The route stays `/` on both sides, so
//! the two revisions line up on the same measurement point.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use wvq_command_bus::{LiveService, QualityService, RunCommand, VerifyCommand};

/// Chromium is expensive and the fixtures bind loopback ports; serialise them.
mod browser_lock;

use browser_lock::BrowserLock;
static NEXT_TEMP_REPO: AtomicU32 = AtomicU32::new(0);

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

fn unique_temp_repo(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let sequence = NEXT_TEMP_REPO.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{nanos}-{sequence}",
        std::process::id()
    ))
}

fn git(root: &Path, args: &[&str]) {
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
}

fn commit(root: &Path, message: &str) {
    git(root, &["add", "-A"]);
    git(
        root,
        &[
            "-c",
            "user.name=WVQ Test",
            "-c",
            "user.email=wvq@example.invalid",
            "commit",
            "-qm",
            message,
        ],
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
            .expect("PowerShell starts");
        assert!(
            output.status.success(),
            "create junction: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(source, target).unwrap();
}

// ---------------------------------------------------------------------------
// A tiny static server, one build of the app per port.
// ---------------------------------------------------------------------------

struct PageServer {
    port: u16,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl PageServer {
    fn start(html: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener.set_nonblocking(true).expect("non-blocking");
        let port = listener.local_addr().expect("addr").port();
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            while !server_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        thread::spawn(move || respond(stream, html));
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            port,
            stop,
            handle: Some(handle),
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
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

fn respond(mut stream: TcpStream, html: &str) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut request = [0_u8; 4096];
    let _ = stream.read(&mut request);
    let body = html.as_bytes();
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

// ---------------------------------------------------------------------------
// The fixture application. Base is healthy; each head variant breaks one thing.
// ---------------------------------------------------------------------------

/// Shared chrome: repeated row actions (scoped), an allowed tooltip overlap,
/// a roomy label, and the Export button the acceptance scenario is about.
const BODY: &str = r#"
  <h1>WVQ checkout</h1>
  <div data-testid="dialog" data-entity="dialog:settings">
    <button data-testid="save" id="save">Save</button>
  </div>

  <button data-testid="export" id="export"
          style="position:absolute;left:400px;top:400px;width:140px;height:44px">Export</button>

  <ul style="list-style:none;padding:0">
    <li data-entity="order:1"><button>Delete</button></li>
    <li data-entity="order:2"><button>Delete</button></li>
    <li data-entity="order:3"><button>Delete</button></li>
  </ul>

  <button data-testid="help" role="button"
          style="position:absolute;left:20px;top:220px;width:40px;height:40px">?</button>
  <div role="tooltip" data-component="Tooltip"
       style="position:absolute;left:10px;top:215px;width:200px;height:50px">Need a hand?</div>

  <div data-testid="total" style="width:400px">Order total: 42.00</div>
"#;

fn page(extra: &str) -> String {
    format!(
        "<!doctype html><html><head><title>WVQ</title></head>\
         <body style=\"margin:0\">{BODY}{extra}</body></html>"
    )
}

/// Leak one page body for the lifetime of the process so the server thread can
/// hold a `'static` reference without a channel.
fn leak(html: String) -> &'static str {
    Box::leak(html.into_boxed_str())
}

fn base_page() -> &'static str {
    leak(page(""))
}

/// Variant B: a nearly transparent overlay covers Export.
fn overlay_page() -> &'static str {
    leak(page(
        r#"<div id="veil" data-testid="veil"
             style="position:absolute;left:380px;top:380px;width:200px;height:100px;
                    background:rgba(0,0,0,0.01)"></div>"#,
    ))
}

/// Variant A: a second Save inside the same dialog scope.
fn duplicate_save_page() -> &'static str {
    leak(page(
        r"<script>
             const dialog = document.querySelector('[data-testid=dialog]');
             const clone = document.createElement('button');
             clone.setAttribute('data-testid', 'save');
             clone.id = 'save';
             clone.textContent = 'Save';
             dialog.appendChild(clone);
           </script>",
    ))
}

/// Variant C: content wider than a 767px viewport.
fn overflow_page() -> &'static str {
    leak(page(
        r#"<style>
              #responsive-menu { position:absolute;left:100px;top:500px;width:140px;height:40px }
              @media (max-width: 767px) { #responsive-menu { left:900px } }
            </style>
            <button id="responsive-menu" data-testid="responsive-menu">Menu</button>"#,
    ))
}

/// Variant D: a critical control whose label no longer fits and has no
/// accessible name to fall back on.
fn clipped_page() -> &'static str {
    leak(page(
        r#"<button id="pay"
             style="position:absolute;left:20px;top:320px;width:44px;height:30px;
                    overflow:hidden;white-space:nowrap">Pay the outstanding balance now</button>"#,
    ))
}

// ---------------------------------------------------------------------------
// Repository fixture
// ---------------------------------------------------------------------------

const PROGRAM: &str = r#"{
  "schema_v": 1,
  "id": "checkout-ui",
  "source": "authored",
  "obligations": ["export-usable"],
  "steps": [
    {"action": "navigate", "route": "/"},
    {"action": "assert", "obligation": "export-usable"}
  ],
  "evidence_policy": {
    "screenshot": "on_failure",
    "trace": "never",
    "network": "always",
    "console": "always",
    "storage": "never"
  }
}"#;

/// The sealed obligation only requires Export to be *visible*. That is the
/// point: the behavioural oracle keeps passing while the control becomes
/// unusable, and only the UI-integrity axis notices.
const QUALITY: &str = r"quality_contract_v: 1
change: checkout-ui

risk:
  default: high

requirements:
  - capability: ui
    requirement: checkout
    scenarios:
      - scenario: export
        obligations:
          - id: export-usable
            kind: behavioral
            expected:
              kind: visible
              target:
                test_id: export
        evidence:
          required: [dom]
          on_failure: [screenshot]
";

fn config(base_url: &str) -> String {
    format!(
        "quality_policy_v: 1\n\n\
         browser:\n  base_url: {base_url}\n  engine: chromium\n  headless: true\n  \
         timeout_ms: 120000\n  module_root: node_modules/playwright\n  programs:\n    \
         - .weavatrix-quality/programs/checkout.json\n\n\
         ui_integrity:\n  enabled: true\n  max_nodes: 2000\n  geometry_tolerance_px: 1\n  \
         occlusion_failure_ratio: 0.5\n  allowed_overlaps:\n    - top:\n        role: tooltip\n      \
         bottom:\n        role: button\n      reason: tooltips intentionally cover their trigger\n"
    )
}

/// A git repository whose base commit talks to `base_url`.
fn checkout_repo(base_url: &str) -> TempRepo {
    let root = unique_temp_repo("wvq-ui-checkout");
    std::fs::create_dir_all(root.join("openspec/changes/checkout-ui/specs/ui")).unwrap();
    std::fs::create_dir_all(root.join(".weavatrix-quality/programs")).unwrap();
    std::fs::write(
        root.join(".gitignore"),
        "node_modules/\n.weavatrix-quality/*.db*\n.weavatrix-quality/cas/\n\
         .weavatrix-quality/runtime/\n.weavatrix-quality/browser-evidence/\n",
    )
    .unwrap();
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"wvq-ui-checkout","private":true,"dependencies":{"playwright":"1.62.1"}}"#,
    )
    .unwrap();
    link_node_modules(&root);
    std::fs::write(
        root.join("openspec/changes/checkout-ui/specs/ui/spec.md"),
        "# Delta for UI\n\n## ADDED Requirements\n\n### Requirement: Checkout\n\
         The system SHALL let a customer export their order.\n\n#### Scenario: Export\n\
         - GIVEN the checkout page\n- WHEN it loads\n- THEN the Export control is usable\n",
    )
    .unwrap();
    std::fs::write(
        root.join("openspec/changes/checkout-ui/quality.yaml"),
        QUALITY,
    )
    .unwrap();
    std::fs::write(
        root.join(".weavatrix-quality/programs/checkout.json"),
        PROGRAM,
    )
    .unwrap();
    std::fs::write(
        root.join(".weavatrix-quality/config.yaml"),
        config(base_url),
    )
    .unwrap();
    git(&root, &["init", "-q"]);
    commit(&root, "baseline");
    TempRepo(root)
}

/// Point the working tree at the head build. In a real repository this is the
/// frontend change itself; here the two builds are two ports.
fn switch_to_head(root: &Path, head_url: &str) {
    std::fs::write(
        root.join(".weavatrix-quality/config.yaml"),
        config(head_url),
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// The acceptance scenario
// ---------------------------------------------------------------------------

#[test]
#[allow(clippy::too_many_lines)]
fn an_overlay_that_blocks_export_blocks_the_change_end_to_end() {
    let _guard = BrowserLock::acquire();

    let base_server = PageServer::start(base_page());
    let head_server = PageServer::start(overlay_page());
    let repo = checkout_repo(&base_server.url());
    let service = LiveService::new(&repo.0);

    // The sealed behavioural test passes on base.
    let base_run = service
        .run(&RunCommand {
            change: "checkout-ui".into(),
            base: "HEAD".into(),
            head: "WORKTREE".into(),
            scope: "all".into(),
            evidence_policy: "standard".into(),
        })
        .unwrap();
    assert_eq!(base_run.outcome, "passed", "base is healthy");

    // The change lands: head now serves the overlay build.
    switch_to_head(&repo.0, &head_server.url());

    let delta = service
        .ui_integrity_view("checkout-ui", "HEAD", "WORKTREE")
        .unwrap();

    // The behavioural oracle still holds: Export is visible.
    let verified = service
        .verify(&VerifyCommand {
            change: "checkout-ui".into(),
        })
        .unwrap();
    assert_eq!(
        verified.verdict, "PROVEN",
        "the sealed test still passes: {:?}",
        verified.proofs
    );

    // ...and the change is blocked anyway, because Export cannot be used.
    assert_eq!(
        verified.state, "BLOCKED",
        "a passing behavioural test must not hide a new occlusion: {:?}",
        verified.quality.blocking_reasons
    );
    assert!(verified.blocking);
    assert_eq!(verified.exit_code(), 2);

    // One snapshot is taken after every step, so a defect that persists across
    // the flow is measured at each point. Base and head therefore always cover
    // the same set of measurement points, which is what makes the comparison
    // sound; the repetition is honest rather than deduplicated away.
    let occlusions: Vec<_> = delta
        .new
        .iter()
        .filter(|item| item.check == wvq_ui::UiCheck::InteractiveOcclusion)
        .collect();
    assert!(
        !occlusions.is_empty(),
        "the overlay must be reported: {}",
        describe(&delta)
    );
    assert!(
        occlusions
            .iter()
            .all(|item| item.subject == "testid:export"),
        "only Export is covered: {}",
        describe(&delta)
    );
    let occlusion = occlusions[0];
    assert_eq!(occlusion.subject, "testid:export");
    assert_eq!(occlusion.viewport, "1280x720");
    assert_eq!(occlusion.route, "/");
    assert_eq!(
        occlusion.evidence.received_event_samples, 0,
        "every probe point was intercepted"
    );
    assert!(occlusion.evidence.sample_count >= 5);
    assert_eq!(occlusion.evidence.failure_ratio_permille, 1000);
    assert_eq!(
        occlusion.counterpart.as_deref(),
        Some("testid:veil"),
        "the exact occluder is named"
    );

    // The base revision had no occlusion at all.
    assert!(
        !delta
            .existing
            .iter()
            .any(|item| item.check == wvq_ui::UiCheck::InteractiveOcclusion),
        "base was clean: {:?}",
        delta.existing
    );

    // The verdict carries the same fact with its provenance.
    let reason = verified
        .quality
        .blocking_reasons
        .iter()
        .find(|item| item.axis == "ui_integrity")
        .expect("a UI reason");
    assert_eq!(reason.rank, 3);
    assert_eq!(reason.subject, "testid:export");
    assert!(reason.detail.contains("WVQ-UI-LAYOUT-001"));
    assert!(reason.detail.contains("1280x720"));
    assert_eq!(verified.quality.ui_integrity.state.as_str(), "blocking");

    // quality_explain resolves the finding to exact numbers, not an opinion.
    let explained = service
        .explain(&wvq_command_bus::ExplainCommand {
            id: occlusion.fingerprint(),
        })
        .unwrap();
    assert_eq!(explained.kind, "ui_finding");
    let provenance = explained.provenance.join("\n");
    assert!(
        provenance.contains("check WVQ-UI-LAYOUT-001"),
        "{provenance}"
    );
    assert!(provenance.contains("target testid:export"), "{provenance}");
    assert!(
        provenance.contains("counterpart testid:veil"),
        "{provenance}"
    );
    assert!(provenance.contains("route / at 1280x720"), "{provenance}");
    assert!(
        provenance.contains("points received events"),
        "the explanation is quantitative: {provenance}"
    );
    assert!(
        provenance.contains("artifact ui-layout-snapshot"),
        "large evidence stays a handle: {provenance}"
    );

    // Zero runtime model tokens on the whole path.
    assert_eq!(verified.quality.ai.runtime_tokens, 0);
    assert_eq!(verified.quality.ai.state.as_str(), "not_applicable");
}

#[test]
fn adaptive_search_blocks_a_regression_that_default_desktop_width_misses() {
    let _guard = BrowserLock::acquire();
    let base_server = PageServer::start(base_page());
    let head_server = PageServer::start(overflow_page());
    let repo = checkout_repo(&base_server.url());
    switch_to_head(&repo.0, &head_server.url());
    let service = LiveService::new(&repo.0);

    let delta = service
        .ui_integrity_view("checkout-ui", "HEAD", "WORKTREE")
        .unwrap();
    assert!(
        delta.new.iter().all(|finding| {
            finding.subject != "testid:responsive-menu" || finding.viewport != "1280x720"
        }),
        "the default desktop run must be clean for the responsive control"
    );
    let interval = delta
        .responsive_intervals
        .iter()
        .find(|interval| {
            interval.finding.check == wvq_ui::UiCheck::ViewportOverflow
                && interval.finding.subject == "testid:responsive-menu"
        })
        .unwrap_or_else(|| panic!("responsive interval missing: {}", describe(&delta)));
    assert_eq!((interval.first_width, interval.last_width), (320, 767));
    assert!(interval.lower_boundary_exact);
    assert!(interval.upper_boundary_exact);
    assert!(!delta.responsive_truncated);

    let verified = service
        .verify(&VerifyCommand {
            change: "checkout-ui".into(),
        })
        .unwrap();
    assert_eq!(verified.state, "BLOCKED");
    assert!(verified.quality.blocking_reasons.iter().any(|reason| {
        reason.axis == "ui_integrity"
            && reason.subject == "testid:responsive-menu"
            && reason.detail.contains("320-767x720")
    }));
}

// ---------------------------------------------------------------------------
// The remaining head variants, against the same base
// ---------------------------------------------------------------------------

/// Collect base and head with real Chromium and classify with the real ratchet.
///
/// This skips the worktree replay that the acceptance test exercises and drives
/// the same collector and detectors directly, so the five remaining variants
/// cost one browser run each instead of two plus a Weavatrix analysis.
fn classify(head: &'static str, viewport: (u32, u32)) -> wvq_ui::UiIntegrityDelta {
    use std::collections::BTreeSet;
    use wvq_runtime::{
        BrowserRunConfig, BrowserViewport, ProgramOracle, TestProgram, UiCollectionConfig,
        run_browser_program_at,
    };

    let policy_yaml = "enabled: true\nmax_nodes: 2000\nallowed_overlaps:\n  - top:\n      \
                       role: tooltip\n    bottom:\n      role: button\n    reason: intentional\n";
    let policy =
        wvq_ui::parse_policy(&serde_yaml::from_str(policy_yaml).unwrap(), "2026-08-23").unwrap();

    let program: TestProgram = TestProgram::from_json(PROGRAM).unwrap();
    let oracles = vec![ProgramOracle {
        obligation: wvq_domain::ObligationId::new("export-usable").unwrap(),
        condition: None,
        expected: serde_json::json!({"kind": "visible", "target": {"test_id": "export"}}),
    }];

    let collect = |html: &'static str, revision: &str| -> wvq_ui::UiIntegritySnapshot {
        let server = PageServer::start(html);
        let dir = unique_temp_repo("wvq-ui-variant");
        std::fs::create_dir_all(&dir).unwrap();
        let result = run_browser_program_at(
            &BrowserRunConfig {
                base_url: server.url(),
                browser: "chromium".into(),
                headless: true,
                timeout: Duration::from_secs(120),
                module_root: Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../..")
                    .join("js/playwright-runner"),
                runtime_dir: dir.join("runtime"),
                evidence_dir: dir.join("evidence"),
                viewport: Some(BrowserViewport {
                    width: viewport.0,
                    height: viewport.1,
                }),
                ui_integrity: Some(UiCollectionConfig {
                    enabled: true,
                    max_nodes: 2_000,
                    ..UiCollectionConfig::default()
                }),
                cancel: Arc::new(AtomicBool::new(false)),
            },
            &program,
            &oracles,
            revision,
        )
        .unwrap();
        let mut snapshot = wvq_ui::UiIntegritySnapshot {
            revision: revision.to_owned(),
            ..wvq_ui::UiIntegritySnapshot::default()
        };
        for evidence in &result.ui_snapshots {
            if !evidence.limitations.is_empty() {
                snapshot.truncated = true;
            }
            if evidence.snapshot.is_null() {
                continue;
            }
            let layout: wvq_ui::LayoutSnapshot =
                serde_json::from_value(evidence.snapshot.clone()).unwrap();
            assert_eq!(layout.viewport.width, viewport.0);
            assert_eq!(layout.viewport.height, viewport.1);
            let output = wvq_ui::detect(&layout, &policy).unwrap();
            snapshot.truncated |= output.truncated;
            snapshot.measured_states.insert(layout.state_key());
            snapshot.findings.extend(output.findings);
        }
        let _ = std::fs::remove_dir_all(&dir);
        snapshot
    };

    let base = collect(base_page(), "rev-base");
    let head_snapshot = collect(head, "rev-head");
    wvq_ui::ratchet(&base, &head_snapshot, &BTreeSet::new(), &policy)
}

fn new_checks(delta: &wvq_ui::UiIntegrityDelta) -> Vec<(&'static str, String)> {
    delta
        .new
        .iter()
        .map(|item| (item.check.id(), item.subject.clone()))
        .collect()
}

/// Everything the ratchet decided, for assertion messages.
fn describe(delta: &wvq_ui::UiIntegrityDelta) -> String {
    let bucket = |label: &str, items: &[wvq_ui::UiIntegrityFinding]| {
        format!(
            "{label}: [{}]",
            items
                .iter()
                .map(|item| format!("{} {}", item.check.id(), item.subject))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "{}; {}; {}; unmeasured: {:?}; truncated: {}",
        bucket("new", &delta.new),
        bucket("existing", &delta.existing),
        bucket("fixed", &delta.fixed),
        delta.unmeasured_states,
        delta.truncated
    )
}

#[test]
fn variant_a_duplicate_save_in_one_dialog_is_new() {
    let _guard = BrowserLock::acquire();
    let delta = classify(duplicate_save_page(), (1280, 720));
    let found = new_checks(&delta);
    assert!(
        found
            .iter()
            .any(|(check, subject)| *check == "WVQ-UI-DUP-001" && subject == "#save"),
        "{}",
        describe(&delta)
    );
    assert!(
        found
            .iter()
            .any(|(check, subject)| *check == "WVQ-UI-DUP-002" && subject == "testid:save"),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|(check, subject)| *check == "WVQ-UI-DUP-003" && subject == "button:Save"),
        "two Save buttons in one dialog scope are ambiguous: {found:?}"
    );
    assert!(delta.blocks());
}

#[test]
fn variant_c_horizontal_overflow_at_767px_is_new() {
    let _guard = BrowserLock::acquire();
    let delta = classify(overflow_page(), (767, 900));
    let found = new_checks(&delta);
    assert!(
        found
            .iter()
            .any(|(check, subject)| *check == "WVQ-UI-LAYOUT-002"
                && subject == "testid:responsive-menu"),
        "the responsive menu must leave a 767px viewport: {found:?}"
    );
}

/// A control whose label no longer fits is reported with its measurements.
///
/// It is a warning rather than a gate here, and deliberately so: the button's
/// DOM text is intact, so assistive technology still announces the whole label
/// and the defect is visual. The gate is reserved for the case where the full
/// value is gone entirely, which `wvq-ui`'s
/// `a_clipped_critical_label_with_no_accessible_value_is_an_error` covers.
#[test]
fn variant_d_a_clipped_critical_label_is_reported_with_its_measurements() {
    let _guard = BrowserLock::acquire();
    let delta = classify(clipped_page(), (1280, 720));
    let clipped = delta
        .new
        .iter()
        .find(|item| item.check == wvq_ui::UiCheck::TextClipping && item.subject == "#pay")
        .unwrap_or_else(|| panic!("no clipping finding: {}", describe(&delta)));
    assert!(
        clipped.evidence.scroll_width > clipped.evidence.client_width,
        "{} vs {}",
        clipped.evidence.scroll_width,
        clipped.evidence.client_width
    );
    assert_eq!(clipped.severity, wvq_domain::Severity::Warn);
    assert!(
        !delta.blocks(),
        "a clipped label whose full text still reaches assistive technology is not a gate"
    );
}

#[test]
fn variants_e_and_f_stay_clean() {
    let _guard = BrowserLock::acquire();
    // The base page already contains both: three Delete buttons in three row
    // scopes, and a tooltip covering its trigger with a declared allowance.
    let delta = classify(base_page(), (1280, 720));
    assert!(
        delta.new.is_empty(),
        "repeated row actions and a declared tooltip overlap are not regressions: {:?}",
        new_checks(&delta)
    );
    assert!(!delta.blocks());
    assert!(
        !delta.truncated,
        "a static page must settle: {:?}",
        delta.unmeasured_states
    );
}
