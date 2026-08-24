//! Actual Rust -> stdio -> Playwright vertical slice.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod browser_lock;

use browser_lock::BrowserLock;
use serde_json::json;
use wvq_domain::{ObligationId, ProgramId};
use wvq_runtime::{
    BrowserAssertionStatus, BrowserRecordingRequest, BrowserRunConfig, EvidencePolicy,
    ProgramOracle, ProgramSource, Target, TestAction, TestProgram, duplicate_mutation_requests,
    record_browser_session, run_browser_program,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("wvq-rust-browser-{nanos}"));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn package_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("js/playwright-runner")
}

#[test]
#[allow(clippy::too_many_lines)]
fn rust_host_executes_a_real_playwright_program() {
    let _guard = BrowserLock::acquire();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let server = thread::spawn(move || {
        while !server_stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _)) => {
                    thread::spawn(move || respond(stream));
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(err) => panic!("test server: {err}"),
            }
        }
    });
    let obligation = ObligationId::new("heading-visible").unwrap();
    let program = TestProgram {
        schema_v: 1,
        id: ProgramId::new("real-playwright-heading").unwrap(),
        source: ProgramSource::Authored,
        obligations: vec![obligation.clone()],
        preconditions: Vec::new(),
        steps: vec![
            TestAction::Navigate { route: "/".into() },
            TestAction::Activate {
                target: Target {
                    role: Some("button".into()),
                    accessible_name: Some("Save".into()),
                    ..Target::default()
                },
            },
            TestAction::Assert {
                obligation: obligation.clone(),
            },
        ],
        data: BTreeMap::new(),
        faults: BTreeMap::new(),
        api_operations: BTreeMap::new(),
        evidence_policy: EvidencePolicy::default(),
        deterministic_seed: Some(7),
    };
    let temp = TempDir::new();
    let result = run_browser_program(
        &BrowserRunConfig {
            base_url: format!("http://{address}"),
            browser: "chromium".into(),
            headless: true,
            timeout: Duration::from_secs(120),
            module_root: package_root(),
            runtime_dir: temp.0.join("runtime"),
            evidence_dir: temp.0.join("evidence"),
            viewport: None,
            ui_integrity: None,
            cancel: Arc::new(AtomicBool::new(false)),
        },
        &program,
        &[ProgramOracle {
            obligation,
            condition: None,
            expected: json!({
                "kind": "visible",
                "target": {"role": "heading", "accessible_name": "WVQ real browser"}
            }),
        }],
    )
    .unwrap();
    assert!(result.passed, "{result:?}");
    assert_eq!(result.asserted, vec!["heading-visible"]);
    assert!(result.contradicted.is_empty());
    assert_eq!(result.observations.len(), 4);
    assert_eq!(result.observations[1].route.as_deref(), Some("/"));
    assert_eq!(result.action_spans.len(), 3);
    assert_eq!(result.action_spans[0].start_observation, 0);
    assert_eq!(result.action_spans[0].end_observation, 1);
    assert_eq!(result.action_spans[1].start_observation, 1);
    assert_eq!(result.action_spans[1].end_observation, 2);
    assert_eq!(result.action_spans[2].start_observation, 2);
    assert_eq!(result.action_spans[2].end_observation, 3);
    assert_eq!(result.assertions.len(), 1);
    assert_eq!(result.assertions[0].step, 2);
    assert_eq!(result.assertions[0].observation, 3);
    assert_eq!(result.assertions[0].status, BrowserAssertionStatus::Passed);
    let duplicates = duplicate_mutation_requests(&result);
    assert_eq!(duplicates.len(), 1, "{result:#?}");
    assert_eq!(duplicates[0].step, 1);
    assert_eq!(duplicates[0].method, "POST");
    assert_eq!(duplicates[0].url, "/api/save");
    assert_eq!(duplicates[0].sequences.len(), 2);

    let condition_missing = run_browser_program(
        &BrowserRunConfig {
            base_url: format!("http://{address}"),
            browser: "chromium".into(),
            headless: true,
            timeout: Duration::from_secs(120),
            module_root: package_root(),
            runtime_dir: temp.0.join("runtime-condition"),
            evidence_dir: temp.0.join("evidence-condition"),
            viewport: None,
            ui_integrity: None,
            cancel: Arc::new(AtomicBool::new(false)),
        },
        &program,
        &[ProgramOracle {
            obligation: ObligationId::new("heading-visible").unwrap(),
            condition: Some(json!({"kind": "route_equals", "value": "/never"})),
            expected: json!({"kind": "no_console_errors"}),
        }],
    )
    .unwrap();
    assert!(!condition_missing.passed);
    assert!(condition_missing.asserted.is_empty());
    assert!(
        condition_missing.contradicted.is_empty(),
        "a missing condition is not a business contradiction: {condition_missing:?}"
    );
    assert!(
        condition_missing
            .failure
            .as_deref()
            .is_some_and(|message| message.starts_with("condition_not_established:"))
    );
    assert_eq!(condition_missing.assertions.len(), 1);
    assert_eq!(
        condition_missing.assertions[0].status,
        BrowserAssertionStatus::Failed
    );

    stop.store(true, Ordering::Release);
    server.join().unwrap();
}

#[test]
fn rust_host_records_a_real_semantic_session_without_raw_form_values() {
    let _guard = BrowserLock::acquire();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let server = thread::spawn(move || {
        while !server_stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _)) => {
                    thread::spawn(move || respond_recording(stream));
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(err) => panic!("recording test server: {err}"),
            }
        }
    });
    let temp = TempDir::new();
    let result = record_browser_session(
        &BrowserRunConfig {
            base_url: format!("http://{address}"),
            browser: "chromium".into(),
            headless: true,
            timeout: Duration::from_secs(30),
            module_root: package_root(),
            runtime_dir: temp.0.join("runtime-record"),
            evidence_dir: temp.0.join("evidence-record"),
            viewport: None,
            ui_integrity: None,
            cancel: Arc::new(AtomicBool::new(false)),
        },
        &BrowserRecordingRequest {
            session: "rust-passive-recording".into(),
            route: "/".into(),
            fixture_values: [("name".into(), "Alice".into())].into(),
            idle_timeout: Duration::from_secs(2),
            max_events: 20,
        },
        &[ProgramOracle {
            obligation: ObligationId::new("details-visible").unwrap(),
            condition: None,
            expected: json!({
                "kind": "text_equals",
                "target": {"role": "status"},
                "value": "Details for Alice"
            }),
        }],
    )
    .unwrap();
    assert_eq!(result.initial.route.as_deref(), Some("blank"));
    assert_eq!(result.events.len(), 3, "{result:#?}");
    assert!(matches!(
        result.events[0].action,
        TestAction::Navigate { .. }
    ));
    assert!(matches!(
        &result.events[1].action,
        TestAction::Fill { value, .. } if value == "name"
    ));
    assert!(matches!(
        result.events[2].action,
        TestAction::Activate { .. }
    ));
    assert!(
        result
            .limitations
            .iter()
            .any(|item| item.contains("has no named fixture"))
    );
    assert!(
        !serde_json::to_string(&result.events)
            .unwrap()
            .contains("s3cr3t-private")
    );
    assert_eq!(result.obligations.len(), 1);
    assert_eq!(result.obligations[0].status, "passed");

    stop.store(true, Ordering::Release);
    server.join().unwrap();
}

fn respond(mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut request = [0_u8; 4096];
    let _ = stream.read(&mut request);
    let body = br"<!doctype html><html><body><h1>WVQ real browser</h1>
        <button>Save</button><script>
          document.querySelector('button').addEventListener('click', () => {
            fetch('/api/save', {method: 'POST'})
              .then(() => fetch('/api/save', {method: 'POST'}));
          });
        </script></body></html>";
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(body).unwrap();
    stream.flush().unwrap();
}

fn respond_recording(mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut request = [0_u8; 4096];
    let _ = stream.read(&mut request);
    let body = br"<!doctype html><html><body>
        <label>Name <input id='name'></label><label>Secret <input id='secret'></label>
        <button data-testid='open-details'>Open details</button><section role='status' hidden></section>
        <script>
          document.querySelector('button').addEventListener('click', () => {
            const status = document.querySelector('[role=status]');
            status.hidden = false;
            status.textContent = 'Details for ' + document.querySelector('#name').value;
          });
          setTimeout(() => {
            const name = document.querySelector('#name'); name.value = 'Alice';
            name.dispatchEvent(new Event('change', {bubbles:true}));
            const secret = document.querySelector('#secret'); secret.value = 's3cr3t-private';
            secret.dispatchEvent(new Event('change', {bubbles:true}));
            document.querySelector('button').click();
            setTimeout(() => document.dispatchEvent(new KeyboardEvent('keydown', {
              key:'E', ctrlKey:true, shiftKey:true, bubbles:true
            })), 150);
          }, 100);
        </script></body></html>";
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(body).unwrap();
    stream.flush().unwrap();
}
