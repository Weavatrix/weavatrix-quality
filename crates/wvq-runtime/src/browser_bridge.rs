//! Bounded Rust host for the bundled thin Playwright bridge.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use wvq_domain::ObligationId;

use crate::process::{TreeChild, spawn_tree};
use crate::{Observation, Target, TestAction, TestProgram};

const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 1024 * 1024;
const BRIDGE_FILES: [(&str, &str); 7] = [
    (
        "main.js",
        include_str!("../../../js/playwright-runner/dist/main.js"),
    ),
    (
        "ui_integrity.js",
        include_str!("../../../js/playwright-runner/dist/ui_integrity.js"),
    ),
    (
        "protocol.js",
        include_str!("../../../js/playwright-runner/dist/protocol.js"),
    ),
    (
        "execute.js",
        include_str!("../../../js/playwright-runner/dist/execute.js"),
    ),
    (
        "observe.js",
        include_str!("../../../js/playwright-runner/dist/observe.js"),
    ),
    (
        "record.js",
        include_str!("../../../js/playwright-runner/dist/record.js"),
    ),
    (
        "playwright.js",
        include_str!("../../../js/playwright-runner/dist/playwright.js"),
    ),
];

/// Fixed browser launch and repository paths supplied by local policy.
#[derive(Debug, Clone)]
pub struct BrowserRunConfig {
    /// Application origin.
    pub base_url: String,
    /// `chromium`, `firefox`, or `webkit`.
    pub browser: String,
    /// Headless launch.
    pub headless: bool,
    /// Per-action and whole bridge deadline.
    pub timeout: Duration,
    /// Repository-relative package root containing Playwright.
    pub module_root: PathBuf,
    /// Ignored local directory where the bundled bridge is materialized.
    pub runtime_dir: PathBuf,
    /// Ignored local directory for screenshot/trace files.
    pub evidence_dir: PathBuf,
    /// Browser viewport. `None` uses the bridge default of 1280x720.
    pub viewport: Option<BrowserViewport>,
    /// Deterministic UI-integrity collection. `None` collects nothing.
    pub ui_integrity: Option<UiCollectionConfig>,
    /// Runner-neutral same-origin API record/replay policy.
    pub network: NetworkRunPolicy,
    /// Cooperative cancellation.
    pub cancel: Arc<AtomicBool>,
}

/// Browser API virtualization mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkMode {
    /// Do not intercept API traffic.
    Live,
    /// Use live traffic and capture a bounded redacted JSON profile.
    Record,
    /// Fulfil known API traffic from the profile and abort unknown API calls.
    Replay,
    /// Replay known API traffic and let unknown API calls continue live.
    Hybrid,
}

/// One deterministic same-origin API response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkReplayEntry {
    /// Uppercase request method.
    pub method: String,
    /// Authority-free path and query.
    pub path: String,
    /// HTTP response status.
    pub status: u16,
    /// JSON media type. Headers and cookies are never retained.
    pub content_type: String,
    /// Bounded redacted JSON response.
    pub body: String,
    /// Request media type used for replay identity. Never a body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_content_type: Option<String>,
    /// Canonical request-body digest. The body itself is never stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_body_digest: Option<String>,
    /// GraphQL operation name when the request was GraphQL-shaped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphql_operation_name: Option<String>,
    /// SHA-256 of the whitespace-normalised GraphQL query.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphql_query_digest: Option<String>,
    /// SHA-256 of canonical GraphQL variables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphql_variables_digest: Option<String>,
}

/// Versioned network replay artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkReplayProfile {
    /// Schema version. `1` is method/path only; `2` adds privacy-safe request identity.
    pub schema_v: u32,
    /// Responses consumed in request order for each method/path identity.
    pub entries: Vec<NetworkReplayEntry>,
}

/// Bounded network policy sent to the thin browser adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetworkRunPolicy {
    /// Live, record, strict replay, or hybrid replay.
    pub mode: NetworkMode,
    /// Required by replay/hybrid; absent for live/record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<NetworkReplayProfile>,
    /// Additional case-insensitive JSON object keys to redact while recording.
    pub redact_json_keys: Vec<String>,
    /// Maximum response entries.
    pub max_entries: u32,
    /// Maximum bytes in one redacted response body.
    pub max_body_bytes: u32,
    /// Maximum bytes across all redacted response bodies.
    pub max_total_bytes: u32,
}

impl Default for NetworkRunPolicy {
    fn default() -> Self {
        Self {
            mode: NetworkMode::Live,
            profile: None,
            redact_json_keys: Vec::new(),
            max_entries: 256,
            max_body_bytes: 64 * 1024,
            max_total_bytes: 4 * 1024 * 1024,
        }
    }
}

impl NetworkRunPolicy {
    fn validate(&self) -> Result<(), BrowserBridgeError> {
        if matches!(self.mode, NetworkMode::Replay | NetworkMode::Hybrid) && self.profile.is_none()
        {
            return Err(BrowserBridgeError::Config(format!(
                "{} network mode requires a replay profile",
                match self.mode {
                    NetworkMode::Replay => "replay",
                    NetworkMode::Hybrid => "hybrid",
                    NetworkMode::Live | NetworkMode::Record => unreachable!(),
                }
            )));
        }
        if !(1..=2_048).contains(&self.max_entries)
            || !(1..=1024 * 1024).contains(&self.max_body_bytes)
            || !(1..=8 * 1024 * 1024).contains(&self.max_total_bytes)
        {
            return Err(BrowserBridgeError::Config(
                "network record/replay bounds are outside the supported ceilings".into(),
            ));
        }
        if self.redact_json_keys.len() > 256
            || self
                .redact_json_keys
                .iter()
                .any(|key| key.trim().is_empty() || key.len() > 128)
        {
            return Err(BrowserBridgeError::Config(
                "network redact_json_keys exceeds its count or name bound".into(),
            ));
        }
        if let Some(profile) = &self.profile {
            profile.validate(self)?;
        }
        Ok(())
    }
}

impl NetworkReplayProfile {
    fn validate(&self, policy: &NetworkRunPolicy) -> Result<(), BrowserBridgeError> {
        if self.schema_v != 1 && self.schema_v != 2 {
            return Err(BrowserBridgeError::Config(format!(
                "unknown network replay schema_v {}",
                self.schema_v
            )));
        }
        if self.entries.len() > policy.max_entries as usize {
            return Err(BrowserBridgeError::Config(format!(
                "network replay profile exceeds the {}-entry ceiling",
                policy.max_entries
            )));
        }
        let mut total = 0_usize;
        for entry in &self.entries {
            if entry.method.is_empty()
                || !entry.method.bytes().all(|byte| byte.is_ascii_uppercase())
                || !entry.path.starts_with('/')
                || entry.path.starts_with("//")
                || entry.path.contains('#')
                || !(100..=599).contains(&entry.status)
                || (entry.content_type != "application/json"
                    && !entry.content_type.ends_with("+json"))
                || serde_json::from_str::<Value>(&entry.body).is_err()
            {
                return Err(BrowserBridgeError::Config(
                    "network replay profile contains an invalid response entry".into(),
                ));
            }
            let bytes = entry.body.len();
            if bytes > policy.max_body_bytes as usize {
                return Err(BrowserBridgeError::Config(
                    "network replay response exceeds its body ceiling".into(),
                ));
            }
            total = total.saturating_add(bytes);
        }
        if total > policy.max_total_bytes as usize {
            return Err(BrowserBridgeError::Config(
                "network replay profile exceeds its total byte ceiling".into(),
            ));
        }
        Ok(())
    }
}

/// Exact browser viewport used for a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BrowserViewport {
    /// CSS viewport width in pixels.
    pub width: u32,
    /// CSS viewport height in pixels.
    pub height: u32,
}

/// Bounds the browser applies while collecting UI-integrity evidence.
///
/// This is transport configuration only. Whether anything collected is a
/// problem is decided in Rust by `wvq-ui`, never in the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UiCollectionConfig {
    /// Whether the collector runs.
    pub enabled: bool,
    /// Ceiling on collected nodes per state.
    pub max_nodes: u32,
    /// Geometry difference between two reads that still counts as settled.
    pub geometry_tolerance_px: u32,
    /// Budget for fonts and a settled layout, in milliseconds.
    pub settle_timeout_ms: u32,
    /// Stable test attribute this project uses.
    pub test_id_attribute: String,
    /// Test ids sealed predicates name, so the collector never drops them.
    pub required_test_ids: Vec<String>,
    /// Full semantic targets named by sealed predicates. Used only to mark
    /// requirement-aware accessibility evidence, never to execute selectors.
    pub required_targets: Vec<Target>,
    /// Discover parsed CSS/container width transitions for adaptive probing.
    pub responsive_breakpoints: bool,
}

impl Default for UiCollectionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_nodes: 5_000,
            geometry_tolerance_px: 1,
            settle_timeout_ms: 2_000,
            test_id_attribute: "data-testid".into(),
            required_test_ids: Vec::new(),
            required_targets: Vec::new(),
            responsive_breakpoints: false,
        }
    }
}

/// One collected layout snapshot plus why it may be incomplete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiSnapshotEvidence {
    /// Zero-based [`TestProgram`] step index the snapshot follows.
    pub step: usize,
    /// Raw `layout_snapshot` document. Decoded and validated by `wvq-ui`.
    pub snapshot: Value,
    /// Bounds hit or instability observed during collection. Non-empty means
    /// the snapshot is not a clean measurement.
    pub limitations: Vec<String>,
    /// Sanitised axe-core / Storybook a11y report, when a producer ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a11y_import: Option<Value>,
}

/// Sealed oracle sent to the deterministic adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProgramOracle {
    /// Obligation identity.
    pub obligation: ObligationId,
    /// Optional sealed precondition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<Value>,
    /// Sealed expected predicate.
    pub expected: Value,
}

/// Exact result of one sealed assertion step and its corresponding observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserAssertionObservation {
    /// Sealed obligation asserted by the step.
    pub obligation: String,
    /// Zero-based [`TestProgram`] step index.
    pub step: usize,
    /// Zero-based observation captured immediately after that step.
    pub observation: usize,
    /// Passed, contradicted, or failed before establishing the expected behavior.
    pub status: BrowserAssertionStatus,
}

/// Exact observation interval owned by one program action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionSpan {
    /// Zero-based step index in the immutable `TestProgram`.
    pub step: usize,
    /// Typed action whose intent owns this interval.
    pub action: TestAction,
    /// Observation immediately before the action.
    pub start_observation: usize,
    /// Observation immediately after the action attempt.
    pub end_observation: usize,
}

/// Repeated mutating request caused inside one user-intent span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateMutationRequest {
    /// Program step that produced the repeated request.
    pub step: usize,
    /// Action from the immutable program.
    pub action: TestAction,
    /// Uppercase mutating method.
    pub method: String,
    /// Authority-free URL identity, retaining path and query.
    pub url: String,
    /// Ordered request identities observed inside this action only.
    pub sequences: Vec<u64>,
}

/// Outcome of an individual browser assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserAssertionStatus {
    /// The sealed expectation was observed.
    Passed,
    /// The sealed expectation was evaluated and contradicted.
    Contradicted,
    /// The assertion could not establish its precondition or otherwise failed.
    Failed,
}

/// One complete browser-program result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserProgramRun {
    /// Program identity.
    pub program: String,
    /// All actions and sealed assertions passed.
    pub passed: bool,
    /// Obligations whose assertion executed successfully.
    pub asserted: Vec<String>,
    /// Obligations whose sealed expectation was contradicted.
    pub contradicted: Vec<String>,
    /// Per-assertion result with exact step and observation identity.
    pub assertions: Vec<BrowserAssertionObservation>,
    /// Initial observation followed by one observation after each attempted step.
    pub observations: Vec<Observation>,
    /// One start/end observation pair per attempted action.
    #[serde(default)]
    pub action_spans: Vec<ActionSpan>,
    /// Screenshot files produced under [`BrowserRunConfig::evidence_dir`].
    pub screenshot_paths: Vec<PathBuf>,
    /// Optional trace file.
    pub trace_path: Option<PathBuf>,
    /// Deterministic UI-integrity snapshots, one per measured step.
    #[serde(default)]
    pub ui_snapshots: Vec<UiSnapshotEvidence>,
    /// Redacted response profile produced only in record mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_profile: Option<NetworkReplayProfile>,
    /// Bounds or response classes that prevented complete recording/replay.
    #[serde(default)]
    pub network_limitations: Vec<String>,
    /// Stable failure text.
    pub failure: Option<String>,
}

/// Bounded inputs for one passive browser recording session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserRecordingRequest {
    /// Stable session identity used only for evidence names.
    pub session: String,
    /// Same-origin root-relative route opened before natural interaction.
    pub route: String,
    /// Explicit safe values mapped to fixture names. Unmatched form values are never captured.
    pub fixture_values: BTreeMap<String, String>,
    /// End the session after this much inactivity.
    pub idle_timeout: Duration,
    /// Hard ceiling for page-originated semantic events.
    pub max_events: u32,
}

/// One semantic page action and the exact observation after it settled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserRecordedEvent {
    /// Typed action; `XPath` and arbitrary JavaScript are not representable.
    pub action: TestAction,
    /// Structured state after this action.
    pub observation: Observation,
}

/// Result of evaluating one existing sealed oracle at the final recorded state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedOracleOutcome {
    /// Existing sealed obligation identity.
    pub obligation: String,
    /// `passed`, `contradicted`, or `condition_not_established`.
    pub status: String,
}

/// A real passive Playwright session before novelty admission or promotion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserRecording {
    /// Initial blank-browser state, before the recorded navigation.
    pub initial: Observation,
    /// Ordered semantic events including the initial navigation.
    pub events: Vec<BrowserRecordedEvent>,
    /// Existing sealed predicates measured at the final state.
    pub obligations: Vec<RecordedOracleOutcome>,
    /// Redacted response profile produced only in record mode.
    pub network_profile: Option<NetworkReplayProfile>,
    /// Redaction, budget, or replay limitations. Never raw form values.
    pub limitations: Vec<String>,
}

/// Find repeated POST/PUT/PATCH/DELETE requests within one action span.
///
/// Identical requests in different spans are separate user intents and are not
/// duplicates. A truncated request journal is ignored here and must be handled
/// by the caller as missing evidence rather than a clean result.
#[must_use]
pub fn duplicate_mutation_requests(run: &BrowserProgramRun) -> Vec<DuplicateMutationRequest> {
    let mut duplicates = Vec::new();
    for span in &run.action_spans {
        let (Some(start), Some(end)) = (
            run.observations.get(span.start_observation),
            run.observations.get(span.end_observation),
        ) else {
            continue;
        };
        if start.network_requests_truncated || end.network_requests_truncated {
            continue;
        }
        let seen = start
            .network_requests
            .iter()
            .map(|request| request.sequence)
            .collect::<BTreeSet<_>>();
        let mut groups: BTreeMap<String, (String, String, Vec<u64>)> = BTreeMap::new();
        for request in end
            .network_requests
            .iter()
            .filter(|request| !seen.contains(&request.sequence))
        {
            let method = request.method.to_ascii_uppercase();
            if !matches!(method.as_str(), "POST" | "PUT" | "PATCH" | "DELETE") {
                continue;
            }
            let path = crate::request_path(&request.url);
            groups
                .entry(request.identity_key())
                .or_insert_with(|| (method, path, Vec::new()))
                .2
                .push(request.sequence);
        }
        for (_, (method, url, sequences)) in groups {
            if sequences.len() > 1 {
                duplicates.push(DuplicateMutationRequest {
                    step: span.step,
                    action: span.action.clone(),
                    method,
                    url,
                    sequences,
                });
            }
        }
    }
    duplicates
}

/// Browser bridge startup, protocol, or evidence failure.
#[derive(Debug, Error)]
pub enum BrowserBridgeError {
    /// Invalid local configuration.
    #[error("invalid browser configuration: {0}")]
    Config(String),
    /// Filesystem/process I/O.
    #[error("browser bridge I/O: {0}")]
    Io(#[from] std::io::Error),
    /// Protocol mismatch or malformed reply.
    #[error("browser bridge protocol: {0}")]
    Protocol(String),
    /// Deadline exceeded.
    #[error("browser bridge deadline exceeded")]
    Deadline,
    /// Cooperative cancellation.
    #[error("browser bridge cancelled")]
    Cancelled,
    /// Bridge rejected a request.
    #[error("browser bridge: {0}")]
    Remote(String),
}

/// Execute one validated program through actual Playwright.
///
/// # Errors
///
/// Returns an error for startup/transport/protocol failures. A browser action
/// or sealed assertion failure is returned as a structured failed run.
pub fn run_browser_program(
    config: &BrowserRunConfig,
    program: &TestProgram,
    oracles: &[ProgramOracle],
) -> Result<BrowserProgramRun, BrowserBridgeError> {
    run_browser_program_at(config, program, oracles, "unknown")
}

/// Record natural same-origin browser use through the bundled Playwright adapter.
///
/// The page can emit only typed semantic actions. Form values leave the page only
/// when they exactly match an explicitly named fixture; unmatched values produce
/// a limitation and are discarded. Existing sealed predicates are evaluated at
/// the final state, but this function does not promote or persist anything.
///
/// # Errors
///
/// Returns an error for invalid bounds, bridge failures, cancellation, or malformed evidence.
pub fn record_browser_session(
    config: &BrowserRunConfig,
    request: &BrowserRecordingRequest,
    oracles: &[ProgramOracle],
) -> Result<BrowserRecording, BrowserBridgeError> {
    validate_config(config)?;
    validate_recording_request(request, config.timeout)?;
    let runner = materialize_bridge(&config.runtime_dir)?;
    let mut bridge = BridgeProcess::spawn(config, &runner)?;
    bridge.request("initialize", &json!({"schema_v": 1}))?;
    let prepared = bridge.request(
        "prepare_recording",
        &json!({
            "session": request.session,
            "route": request.route,
            "fixture_values": request.fixture_values,
            "max_events": request.max_events,
            "oracles": oracles,
            "config": {
                "base_url": config.base_url,
                "browser": config.browser,
                "headless": config.headless,
                "timeout_ms": u64::try_from(config.timeout.as_millis()).unwrap_or(u64::MAX),
                "evidence_dir": config.evidence_dir,
                "viewport": config.viewport,
                "network": config.network,
            }
        }),
    )?;
    let initial =
        serde_json::from_value(prepared.get("initial").cloned().ok_or_else(|| {
            BrowserBridgeError::Protocol("recorder omitted initial state".into())
        })?)
        .map_err(|err| BrowserBridgeError::Protocol(err.to_string()))?;
    let mut events = Vec::new();
    let mut limitations = Vec::new();
    let mut last_event = Instant::now();
    loop {
        let body = bridge.request("poll_recording", &json!({}))?;
        let mut polled: Vec<BrowserRecordedEvent> =
            serde_json::from_value(body.get("events").cloned().unwrap_or_else(|| json!([])))
                .map_err(|err| BrowserBridgeError::Protocol(err.to_string()))?;
        if !polled.is_empty() {
            last_event = Instant::now();
            events.append(&mut polled);
        }
        limitations.extend(
            body.get("limitations")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned),
        );
        let done = body.get("done").and_then(Value::as_bool).unwrap_or(false);
        if done || last_event.elapsed() >= request.idle_timeout {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    let finished = bridge.request("finish_recording", &json!({}))?;
    let obligations = serde_json::from_value(
        finished
            .get("obligations")
            .cloned()
            .unwrap_or_else(|| json!([])),
    )
    .map_err(|err| BrowserBridgeError::Protocol(err.to_string()))?;
    let network_profile: Option<NetworkReplayProfile> = finished
        .get("network_profile")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|err| BrowserBridgeError::Protocol(err.to_string()))?;
    if let Some(profile) = &network_profile {
        profile.validate(&config.network)?;
    }
    limitations.extend(
        finished
            .get("network_limitations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned),
    );
    bridge.close()?;
    limitations.sort();
    limitations.dedup();
    Ok(BrowserRecording {
        initial,
        events,
        obligations,
        network_profile,
        limitations,
    })
}

fn validate_recording_request(
    request: &BrowserRecordingRequest,
    bridge_timeout: Duration,
) -> Result<(), BrowserBridgeError> {
    if request.session.trim().is_empty() || request.session.len() > 128 {
        return Err(BrowserBridgeError::Config(
            "recording session must contain 1..=128 characters".into(),
        ));
    }
    if !request.route.starts_with('/') || request.route.starts_with("//") {
        return Err(BrowserBridgeError::Config(
            "recording route must be same-origin and root-relative".into(),
        ));
    }
    if request.idle_timeout < Duration::from_millis(50)
        || request.idle_timeout > Duration::from_secs(60)
        || request.idle_timeout >= bridge_timeout
    {
        return Err(BrowserBridgeError::Config(
            "recording idle timeout must be 50ms..=60s and shorter than the bridge deadline".into(),
        ));
    }
    if !(1..=1_000).contains(&request.max_events) {
        return Err(BrowserBridgeError::Config(
            "recording max_events must be between 1 and 1000".into(),
        ));
    }
    if request.fixture_values.len() > 256
        || request
            .fixture_values
            .iter()
            .any(|(key, value)| key.trim().is_empty() || key.len() > 128 || value.len() > 8 * 1024)
        || request
            .fixture_values
            .iter()
            .map(|(key, value)| key.len().saturating_add(value.len()))
            .sum::<usize>()
            > 64 * 1024
    {
        return Err(BrowserBridgeError::Config(
            "recording fixture data exceeds its name, value, count, or 64 KiB bound".into(),
        ));
    }
    Ok(())
}

/// Execute one validated program and bind its evidence to an exact revision.
///
/// `revision` is stamped onto every collected UI snapshot so a snapshot can
/// never be compared against the wrong side of a base/head range.
///
/// # Errors
///
/// Same as [`run_browser_program`].
#[allow(clippy::too_many_lines)]
pub fn run_browser_program_at(
    config: &BrowserRunConfig,
    program: &TestProgram,
    oracles: &[ProgramOracle],
    revision: &str,
) -> Result<BrowserProgramRun, BrowserBridgeError> {
    validate_config(config)?;
    program
        .validate()
        .map_err(|err| BrowserBridgeError::Config(err.to_string()))?;
    let known = oracles
        .iter()
        .map(|oracle| oracle.obligation.as_str())
        .collect::<BTreeSet<_>>();
    if let Some(missing) = program
        .obligations
        .iter()
        .find(|obligation| !known.contains(obligation.as_str()))
    {
        return Err(BrowserBridgeError::Config(format!(
            "program {} has no sealed predicate for {missing}",
            program.id
        )));
    }
    let runner = materialize_bridge(&config.runtime_dir)?;
    let mut bridge = BridgeProcess::spawn(config, &runner)?;
    bridge.request("initialize", &json!({"schema_v": 1}))?;
    bridge.request(
        "prepare",
        &json!({
            "program": program,
            "oracles": oracles,
            "config": {
                "base_url": config.base_url,
                "browser": config.browser,
                "headless": config.headless,
                "timeout_ms": u64::try_from(config.timeout.as_millis()).unwrap_or(u64::MAX),
                "evidence_dir": config.evidence_dir,
                "viewport": config.viewport,
                "network": config.network,
            }
        }),
    )?;

    let mut passed = true;
    let mut failure = None;
    let mut asserted = Vec::new();
    let mut contradicted = Vec::new();
    let mut assertions = Vec::new();
    let mut observations = Vec::new();
    let mut action_spans = Vec::new();
    let mut screenshot_paths = Vec::new();
    let mut ui_snapshots = Vec::new();
    let (initial, screenshot) =
        observe_bridge(&mut bridge, false, false, false, &config.evidence_dir)?;
    if let Some(path) = screenshot {
        screenshot_paths.push(path);
    }
    observations.push(initial);
    for (index, step) in program.steps.iter().enumerate() {
        let start_observation = observations.len().saturating_sub(1);
        let step_result = bridge.request("execute_step", &json!({"index": index}));
        let assertion = classify_assertion(step, &step_result, &mut asserted, &mut contradicted);
        if let Err(err) = step_result {
            match err {
                BrowserBridgeError::Remote(message) => {
                    passed = false;
                    failure = Some(message);
                }
                other => return Err(other),
            }
        }
        let (observation, screenshot) =
            observe_bridge(&mut bridge, !passed, true, true, &config.evidence_dir)?;
        if let Some(path) = screenshot {
            screenshot_paths.push(path);
        }
        let state_digest = observation
            .a11y_digest
            .as_deref()
            .unwrap_or("00")
            .to_owned();
        observations.push(observation);
        let end_observation = observations.len().saturating_sub(1);
        action_spans.push(ActionSpan {
            step: index,
            action: step.clone(),
            start_observation,
            end_observation,
        });
        if let Some(ui) = config.ui_integrity.as_ref().filter(|ui| ui.enabled) {
            ui_snapshots.push(collect_ui_snapshot(
                &mut bridge,
                ui,
                index,
                revision,
                &state_digest,
            )?);
        }
        if let Some((obligation, status)) = assertion {
            assertions.push(BrowserAssertionObservation {
                obligation,
                step: index,
                observation: end_observation,
                status,
            });
        }
        if !passed {
            break;
        }
    }
    let finished = bridge.request("finish", &json!({}))?;
    let trace_path = finished
        .get("trace_path")
        .and_then(Value::as_str)
        .map(|path| validated_evidence_path(&config.evidence_dir, path))
        .transpose()?;
    let network_profile: Option<NetworkReplayProfile> = finished
        .get("network_profile")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|err| BrowserBridgeError::Protocol(err.to_string()))?;
    if let Some(profile) = &network_profile {
        profile.validate(&config.network)?;
    }
    let network_limitations: Vec<String> = finished
        .get("network_limitations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect();
    if matches!(config.network.mode, NetworkMode::Replay) && !network_limitations.is_empty() {
        passed = false;
        failure.get_or_insert_with(|| network_limitations[0].clone());
    }
    bridge.close()?;
    screenshot_paths.sort();
    screenshot_paths.dedup();
    Ok(BrowserProgramRun {
        program: program.id.to_string(),
        passed,
        asserted,
        contradicted,
        assertions,
        observations,
        action_spans,
        screenshot_paths,
        trace_path,
        ui_snapshots,
        network_profile,
        network_limitations,
        failure,
    })
}

fn observe_bridge(
    bridge: &mut BridgeProcess,
    failed: bool,
    settle_action: bool,
    capture_screenshot: bool,
    evidence_dir: &Path,
) -> Result<(Observation, Option<PathBuf>), BrowserBridgeError> {
    let body = bridge.request(
        "observe",
        &json!({
            "failed": failed,
            "settle_action": settle_action,
            "capture_screenshot": capture_screenshot
        }),
    )?;
    let screenshot = body
        .get("screenshot_path")
        .and_then(Value::as_str)
        .map(|path| validated_evidence_path(evidence_dir, path))
        .transpose()?;
    let mut observation: Observation = serde_json::from_value(body)
        .map_err(|err| BrowserBridgeError::Protocol(err.to_string()))?;
    if let Some(path) = &screenshot {
        let bytes = std::fs::read(path)?;
        observation.visual_digest = Some(crate::bytes_digest(&bytes));
        observation.visual_surface = Some("screenshot_png".into());
    }
    Ok((observation, screenshot))
}

/// Ask the bridge for one deterministic layout snapshot.
///
/// A collection failure never fails the run: the browser measured the
/// behaviour fine, and losing UI evidence is a gap to report, not a defect to
/// blame. The gap is recorded as a limitation so the axis reports `unmeasured`
/// rather than clean.
fn collect_ui_snapshot(
    bridge: &mut BridgeProcess,
    config: &UiCollectionConfig,
    step: usize,
    revision: &str,
    state_digest: &str,
) -> Result<UiSnapshotEvidence, BrowserBridgeError> {
    let request = bridge.request(
        "collect_ui",
        &json!({
            "step": step,
            "revision": revision,
            "state_digest": state_digest,
            "config": config,
        }),
    );
    match request {
        Ok(body) => {
            let limitations = body
                .get("limitations")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            let snapshot = body.get("snapshot").cloned().ok_or_else(|| {
                BrowserBridgeError::Protocol("collect_ui omitted the layout snapshot".into())
            })?;
            Ok(UiSnapshotEvidence {
                step,
                snapshot,
                limitations,
                a11y_import: body.get("a11y_import").cloned().filter(|value| !value.is_null()),
            })
        }
        Err(BrowserBridgeError::Remote(message)) => Ok(UiSnapshotEvidence {
            step,
            snapshot: Value::Null,
            limitations: vec![format!("UI collection failed at step {step}: {message}")],
            a11y_import: None,
        }),
        Err(other) => Err(other),
    }
}

fn classify_assertion(
    step: &TestAction,
    result: &Result<Value, BrowserBridgeError>,
    asserted: &mut Vec<String>,
    contradicted: &mut Vec<String>,
) -> Option<(String, BrowserAssertionStatus)> {
    let TestAction::Assert { obligation } = step else {
        return None;
    };
    let status = match result {
        Ok(_) => {
            asserted.push(obligation.to_string());
            BrowserAssertionStatus::Passed
        }
        Err(BrowserBridgeError::Remote(message))
            if message.starts_with(&format!("assertion_failed:{obligation}:")) =>
        {
            contradicted.push(obligation.to_string());
            BrowserAssertionStatus::Contradicted
        }
        Err(_) => BrowserAssertionStatus::Failed,
    };
    Some((obligation.to_string(), status))
}

fn validate_config(config: &BrowserRunConfig) -> Result<(), BrowserBridgeError> {
    if !config.base_url.starts_with("http://") && !config.base_url.starts_with("https://") {
        return Err(BrowserBridgeError::Config(
            "base_url must use http or https".into(),
        ));
    }
    if !matches!(config.browser.as_str(), "chromium" | "firefox" | "webkit") {
        return Err(BrowserBridgeError::Config(format!(
            "unknown browser `{}`",
            config.browser
        )));
    }
    if config.timeout.is_zero() || config.timeout > Duration::from_secs(120) {
        return Err(BrowserBridgeError::Config(
            "timeout must be between 1ms and 120s".into(),
        ));
    }
    if let Some(viewport) = config.viewport
        && (!(1..=16_384).contains(&viewport.width) || !(1..=16_384).contains(&viewport.height))
    {
        return Err(BrowserBridgeError::Config(
            "viewport dimensions must be between 1 and 16384 pixels".into(),
        ));
    }
    config.network.validate()?;
    if !config.module_root.is_dir() {
        return Err(BrowserBridgeError::Config(format!(
            "Playwright module root does not exist: {}",
            config.module_root.display()
        )));
    }
    Ok(())
}

fn materialize_bridge(runtime_dir: &Path) -> Result<PathBuf, BrowserBridgeError> {
    std::fs::create_dir_all(runtime_dir)?;
    write_if_changed(
        &runtime_dir.join("package.json"),
        b"{\"type\":\"module\"}\n",
    )?;
    for (name, contents) in BRIDGE_FILES {
        write_if_changed(&runtime_dir.join(name), contents.as_bytes())?;
    }
    Ok(runtime_dir.join("main.js"))
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    if std::fs::read(path).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    std::fs::write(path, bytes)
}

fn validated_evidence_path(root: &Path, raw: &str) -> Result<PathBuf, BrowserBridgeError> {
    let path = PathBuf::from(raw);
    let root = root.canonicalize()?;
    let path = path.canonicalize()?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(BrowserBridgeError::Protocol(format!(
            "bridge evidence escaped the configured directory: {}",
            path.display()
        )));
    }
    Ok(path)
}

struct BridgeProcess {
    child: TreeChild,
    stdin: Option<ChildStdin>,
    replies: mpsc::Receiver<Result<String, String>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    deadline: Instant,
    cancel: Arc<AtomicBool>,
    next_id: u64,
}

impl BridgeProcess {
    fn spawn(config: &BrowserRunConfig, runner: &Path) -> Result<Self, BrowserBridgeError> {
        let mut command = Command::new(if cfg!(windows) { "node.exe" } else { "node" });
        command
            .arg(runner)
            .current_dir(&config.module_root)
            .env("WVQ_PLAYWRIGHT_MODULE_ROOT", &config.module_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = spawn_tree(command)?;
        let stdin = child
            .stdin()
            .take()
            .ok_or_else(|| BrowserBridgeError::Protocol("child stdin was not piped".into()))?;
        let stdout = child
            .stdout()
            .take()
            .ok_or_else(|| BrowserBridgeError::Protocol("child stdout was not piped".into()))?;
        let stderr_pipe = child
            .stderr()
            .take()
            .ok_or_else(|| BrowserBridgeError::Protocol("child stderr was not piped".into()))?;
        let (sender, replies) = mpsc::sync_channel(4);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) if line.len() <= MAX_LINE_BYTES => {
                        if sender.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {
                        let _ = sender.send(Err("reply exceeded 8 MiB".into()));
                        break;
                    }
                    Err(err) => {
                        let _ = sender.send(Err(err.to_string()));
                        break;
                    }
                }
            }
        });
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let stderr_out = Arc::clone(&stderr);
        thread::spawn(move || {
            let mut reader = stderr_pipe.take(u64::try_from(MAX_STDERR_BYTES).unwrap_or(u64::MAX));
            let mut bytes = Vec::new();
            let _ = reader.read_to_end(&mut bytes);
            *stderr_out
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = bytes;
        });
        Ok(Self {
            child,
            stdin: Some(stdin),
            replies,
            stderr,
            deadline: Instant::now() + config.timeout,
            cancel: Arc::clone(&config.cancel),
            next_id: 1,
        })
    }

    fn request(&mut self, method: &str, params: &Value) -> Result<Value, BrowserBridgeError> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(BrowserBridgeError::Cancelled);
        }
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let mut line = serde_json::to_vec(&json!({
            "method": method,
            "id": id,
            "params": params,
        }))
        .map_err(|err| BrowserBridgeError::Protocol(err.to_string()))?;
        if line.len() > MAX_LINE_BYTES {
            return Err(BrowserBridgeError::Protocol(
                "request exceeded 8 MiB".into(),
            ));
        }
        line.push(b'\n');
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| BrowserBridgeError::Protocol("bridge stdin is closed".into()))?;
        stdin.write_all(&line)?;
        stdin.flush()?;
        let raw = loop {
            if self.cancel.load(Ordering::Acquire) {
                return Err(BrowserBridgeError::Cancelled);
            }
            let now = Instant::now();
            if now >= self.deadline {
                return Err(BrowserBridgeError::Deadline);
            }
            let wait = (self.deadline - now).min(Duration::from_millis(100));
            match self.replies.recv_timeout(wait) {
                Ok(Ok(line)) => break line,
                Ok(Err(message)) => return Err(BrowserBridgeError::Protocol(message)),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(BrowserBridgeError::Protocol(self.exit_detail()));
                }
            }
        };
        let reply: BridgeWireReply = serde_json::from_str(raw.trim_end())
            .map_err(|err| BrowserBridgeError::Protocol(err.to_string()))?;
        match reply {
            BridgeWireReply::Ok { id: found, body } if found == id => Ok(body),
            BridgeWireReply::Error { id: found, error } if found == id => {
                Err(BrowserBridgeError::Remote(error))
            }
            other => Err(BrowserBridgeError::Protocol(format!(
                "reply id mismatch: expected {id}, got {}",
                other.id()
            ))),
        }
    }

    fn close(&mut self) -> Result<(), BrowserBridgeError> {
        self.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if self.child.try_wait()?.is_some() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                self.child.kill()?;
                let _ = self.child.wait();
                return Err(BrowserBridgeError::Protocol(
                    "bridge did not exit after finish".into(),
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn exit_detail(&mut self) -> String {
        let status = self.child.try_wait().ok().flatten();
        let stderr = self
            .stderr
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        format!(
            "bridge output closed (status {status:?}): {}",
            String::from_utf8_lossy(&stderr)
        )
    }
}

impl Drop for BridgeProcess {
    fn drop(&mut self) {
        self.stdin.take();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BridgeWireReply {
    Ok {
        id: u64,
        #[serde(default)]
        body: Value,
    },
    Error {
        id: u64,
        error: String,
    },
}

impl BridgeWireReply {
    fn id(&self) -> u64 {
        match self {
            Self::Ok { id, .. } | Self::Error { id, .. } => *id,
        }
    }
}
