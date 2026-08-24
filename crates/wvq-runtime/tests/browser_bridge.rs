//! Actual Rust -> stdio -> Playwright vertical slice.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod browser_lock;

use browser_lock::BrowserLock;
use serde_json::json;
use wvq_domain::{ObligationId, ProgramId};
use wvq_runtime::{
    BrowserAssertionStatus, BrowserRecordingRequest, BrowserRunConfig, EvidencePolicy, NetworkMode,
    NetworkReplayProfile, NetworkRunPolicy, ProgramOracle, ProgramSource, Target, TestAction,
    TestProgram, WaitCondition, duplicate_mutation_requests, record_browser_session,
    run_browser_program,
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
            network: NetworkRunPolicy::default(),
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
            network: NetworkRunPolicy::default(),
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
            network: NetworkRunPolicy::default(),
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

#[test]
#[allow(clippy::too_many_lines)]
fn rust_host_records_redacted_json_and_strictly_replays_it() {
    let _guard = BrowserLock::acquire();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let api_calls = Arc::new(AtomicU64::new(0));
    let server_stop = Arc::clone(&stop);
    let server_calls = Arc::clone(&api_calls);
    let server = thread::spawn(move || {
        while !server_stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let calls = Arc::clone(&server_calls);
                    thread::spawn(move || respond_network_replay(stream, &calls));
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(err) => panic!("network replay server: {err}"),
            }
        }
    });
    let obligation = ObligationId::new("profile-loaded").unwrap();
    let program = TestProgram {
        schema_v: 1,
        id: ProgramId::new("network-replay-profile").unwrap(),
        source: ProgramSource::Authored,
        obligations: vec![obligation.clone()],
        preconditions: Vec::new(),
        steps: vec![
            TestAction::Navigate { route: "/".into() },
            TestAction::Wait {
                condition: WaitCondition::Visible {
                    target: Target {
                        role: Some("status".into()),
                        ..Target::default()
                    },
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
        deterministic_seed: Some(11),
    };
    let oracle = ProgramOracle {
        obligation,
        condition: None,
        expected: json!({
            "kind": "text_equals",
            "target": {"role": "status"},
            "value": "true:7"
        }),
    };
    let temp = TempDir::new();
    let recorded = run_browser_program(
        &BrowserRunConfig {
            base_url: format!("http://{address}"),
            browser: "chromium".into(),
            headless: true,
            timeout: Duration::from_secs(30),
            module_root: package_root(),
            runtime_dir: temp.0.join("runtime-network-record"),
            evidence_dir: temp.0.join("evidence-network-record"),
            viewport: None,
            ui_integrity: None,
            network: NetworkRunPolicy {
                mode: NetworkMode::Record,
                ..NetworkRunPolicy::default()
            },
            cancel: Arc::new(AtomicBool::new(false)),
        },
        &program,
        std::slice::from_ref(&oracle),
    )
    .unwrap();
    assert!(recorded.passed, "{recorded:#?}");
    assert_eq!(api_calls.load(Ordering::Acquire), 1);
    let profile = recorded.network_profile.expect("redacted replay profile");
    assert_eq!(profile.entries.len(), 1);
    let serialized = serde_json::to_string(&profile).unwrap();
    assert!(!serialized.contains("private@example.invalid"));
    assert!(!serialized.contains("secret-token-value"));
    assert!(serialized.contains("[REDACTED]"));

    let replayed = run_browser_program(
        &BrowserRunConfig {
            base_url: format!("http://{address}"),
            browser: "chromium".into(),
            headless: true,
            timeout: Duration::from_secs(30),
            module_root: package_root(),
            runtime_dir: temp.0.join("runtime-network-replay"),
            evidence_dir: temp.0.join("evidence-network-replay"),
            viewport: None,
            ui_integrity: None,
            network: NetworkRunPolicy {
                mode: NetworkMode::Replay,
                profile: Some(profile),
                ..NetworkRunPolicy::default()
            },
            cancel: Arc::new(AtomicBool::new(false)),
        },
        &program,
        &[oracle],
    )
    .unwrap();
    assert!(replayed.passed, "{replayed:#?}");
    assert_eq!(
        api_calls.load(Ordering::Acquire),
        1,
        "strict replay must not reach the API upstream"
    );

    let page_obligation = ObligationId::new("page-visible").unwrap();
    let page_program = TestProgram {
        schema_v: 1,
        id: ProgramId::new("strict-replay-missing-api").unwrap(),
        source: ProgramSource::Authored,
        obligations: vec![page_obligation.clone()],
        preconditions: Vec::new(),
        steps: vec![
            TestAction::Navigate { route: "/".into() },
            TestAction::Assert {
                obligation: page_obligation.clone(),
            },
        ],
        data: BTreeMap::new(),
        faults: BTreeMap::new(),
        api_operations: BTreeMap::new(),
        evidence_policy: EvidencePolicy::default(),
        deterministic_seed: Some(11),
    };
    let missing = run_browser_program(
        &BrowserRunConfig {
            base_url: format!("http://{address}"),
            browser: "chromium".into(),
            headless: true,
            timeout: Duration::from_secs(30),
            module_root: package_root(),
            runtime_dir: temp.0.join("runtime-network-missing"),
            evidence_dir: temp.0.join("evidence-network-missing"),
            viewport: None,
            ui_integrity: None,
            network: NetworkRunPolicy {
                mode: NetworkMode::Replay,
                profile: Some(NetworkReplayProfile {
                    schema_v: 1,
                    entries: Vec::new(),
                }),
                ..NetworkRunPolicy::default()
            },
            cancel: Arc::new(AtomicBool::new(false)),
        },
        &page_program,
        &[ProgramOracle {
            obligation: page_obligation,
            condition: None,
            expected: json!({
                "kind": "visible",
                "target": {"role": "heading", "accessible_name": "Network replay"}
            }),
        }],
    )
    .unwrap();
    assert!(
        !missing.passed,
        "an unrecorded strict API request must fail even when the page oracle passes"
    );
    assert!(
        missing
            .failure
            .as_deref()
            .is_some_and(|failure| failure.contains("strict network replay has no response")),
        "{missing:#?}"
    );

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

fn respond_network_replay(mut stream: TcpStream, api_calls: &AtomicU64) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut request = [0_u8; 4096];
    let count = stream.read(&mut request).unwrap_or_default();
    let request = String::from_utf8_lossy(&request[..count]);
    let is_api = request.starts_with("GET /api/profile ");
    let (content_type, body) = if is_api {
        api_calls.fetch_add(1, Ordering::AcqRel);
        (
            "application/json; charset=utf-8",
            r#"{"ok":true,"version":7,"email":"private@example.invalid","token":"secret-token-value"}"#,
        )
    } else {
        (
            "text/html; charset=utf-8",
            r"<!doctype html><h1>Network replay</h1><script>
              fetch('/api/profile').then(response => response.json()).then(value => {
                const status = document.createElement('p');
                status.setAttribute('role', 'status');
                status.textContent = value.ok + ':' + value.version;
                document.body.appendChild(status);
              });
            </script>",
        )
    };
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(body.as_bytes()).unwrap();
    stream.flush().unwrap();
}
