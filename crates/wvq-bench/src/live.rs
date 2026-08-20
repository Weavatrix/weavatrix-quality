//! Live selected-vs-full execution through the production command bus.

use std::time::Instant;

use serde::Serialize;
use serde_json::Value;
use thiserror::Error;
use wvq_command_bus::{BusError, EvidenceCommand, QualityService, RunCommand, RunReply};

/// A measured production run, not a declared candidate cost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MeasuredRun {
    /// Requested scope (`impacted` or `all`).
    pub requested_scope: String,
    /// Effective scope after fail-closed widening.
    pub effective_scope: String,
    /// Exact command-bus reason the scope was kept or widened.
    pub scope_reason: String,
    /// Base reference resolved by the production service.
    pub base: String,
    /// Head reference resolved by the production service.
    pub head: String,
    /// Production run identity.
    pub run_id: String,
    /// Terminal command-bus status.
    pub status: String,
    /// Whether registered executors were actually invoked.
    pub executed: bool,
    /// Aggregate executor outcome.
    pub outcome: String,
    /// Repository test paths selected for this run.
    pub selected_test_count: u64,
    /// Filterable repository test paths available for this run.
    pub available_test_count: u64,
    /// Real elapsed wall-clock time around command-bus execution and evidence reads.
    pub wall_clock_ms: u64,
    /// Number of evidence handles returned by the run.
    pub artifact_count: u64,
    /// Evidence handles for later drill-down; artifact bodies stay in CAS.
    pub artifact_handles: Vec<String>,
    /// Total bytes stored behind those handles.
    pub artifact_bytes: u64,
    /// Executor invocations recorded in the execution summary, when it was inline.
    pub executor_invocations: Option<u64>,
    /// Browser programs recorded in the execution summary, when it was inline.
    pub browser_programs: Option<u64>,
}

/// Live shadow comparison of impacted selection against the full registered suite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveShadowReport {
    /// Distinguishes this report from synthetic labelled-case evaluation.
    pub measurement_kind: &'static str,
    /// `OpenSpec` change identity.
    pub change: String,
    /// Immutable base reference supplied to both runs.
    pub base: String,
    /// Head reference supplied to both runs.
    pub head: String,
    /// Actual impacted execution.
    pub impacted: MeasuredRun,
    /// Actual full execution.
    pub full: MeasuredRun,
    /// Normal verification never invokes a model.
    pub runtime_llm_tokens: u64,
    /// Both runs completed over the same requested revision range.
    pub comparable: bool,
    /// Whether an impacted run executed fewer repository test paths than were available.
    pub selection_reduced: bool,
}

/// Why a live shadow measurement could not complete.
#[derive(Debug, Error)]
pub enum LiveShadowError {
    /// Evidence policy is outside the command-bus allowlist.
    #[error("evidence policy must be standard, minimal, or none")]
    InvalidEvidencePolicy,
    /// Production execution or evidence retrieval failed.
    #[error(transparent)]
    Bus(#[from] BusError),
    /// A stored execution summary claimed to be JSON but was not valid.
    #[error("execution summary is malformed: {0}")]
    MalformedSummary(serde_json::Error),
}

/// Execute one impacted run and one full run through the same bounded production API.
///
/// The two measurements intentionally run sequentially so they do not compete for
/// CPU, memory, ports, or the repository evidence ledger.
///
/// # Errors
///
/// Fails before execution for an unknown evidence policy and propagates any
/// command-bus, runner, revision, or evidence error.
pub fn run_live_shadow(
    service: &dyn QualityService,
    change: &str,
    base: &str,
    head: &str,
    evidence_policy: &str,
) -> Result<LiveShadowReport, LiveShadowError> {
    if !matches!(evidence_policy, "standard" | "minimal" | "none") {
        return Err(LiveShadowError::InvalidEvidencePolicy);
    }
    let impacted = measure(
        service,
        &RunCommand {
            change: change.to_owned(),
            scope: "impacted".into(),
            evidence_policy: evidence_policy.to_owned(),
            base: base.to_owned(),
            head: head.to_owned(),
        },
    )?;
    let full = measure(
        service,
        &RunCommand {
            change: change.to_owned(),
            scope: "all".into(),
            evidence_policy: evidence_policy.to_owned(),
            base: base.to_owned(),
            head: head.to_owned(),
        },
    )?;
    let comparable = comparable_run(&impacted, base, head) && comparable_run(&full, base, head);
    let selection_reduced = impacted.effective_scope == "impacted"
        && impacted.selected_test_count < impacted.available_test_count;
    Ok(LiveShadowReport {
        measurement_kind: "live_execution",
        change: change.to_owned(),
        base: base.to_owned(),
        head: head.to_owned(),
        impacted,
        full,
        runtime_llm_tokens: 0,
        comparable,
        selection_reduced,
    })
}

fn comparable_run(run: &MeasuredRun, base: &str, head: &str) -> bool {
    run.executed
        && run.status == "complete"
        && run.base == base
        && run.head == head
        && !run.run_id.is_empty()
        && matches!(run.outcome.as_str(), "passed" | "failed" | "error")
}

fn measure(
    service: &dyn QualityService,
    command: &RunCommand,
) -> Result<MeasuredRun, LiveShadowError> {
    let started = Instant::now();
    let reply = service.run(command)?;
    let evidence = evidence_measurement(service, &reply)?;
    let elapsed = started.elapsed();
    let artifact_count = u64::try_from(reply.artifact_handles.len()).unwrap_or(u64::MAX);
    Ok(MeasuredRun {
        requested_scope: reply.requested_scope,
        effective_scope: reply.scope,
        scope_reason: reply.scope_reason,
        base: reply.base,
        head: reply.head,
        run_id: reply.run_id,
        status: reply.status,
        executed: reply.executed,
        outcome: reply.outcome,
        selected_test_count: reply.selected_test_count,
        available_test_count: reply.available_test_count,
        wall_clock_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
        artifact_count,
        artifact_handles: reply.artifact_handles,
        artifact_bytes: evidence.bytes,
        executor_invocations: evidence.executors.or(Some(reply.executor_invocations)),
        browser_programs: evidence.browser_programs.or(Some(reply.browser_programs)),
    })
}

#[derive(Default)]
struct EvidenceMeasurement {
    bytes: u64,
    executors: Option<u64>,
    browser_programs: Option<u64>,
}

fn evidence_measurement(
    service: &dyn QualityService,
    reply: &RunReply,
) -> Result<EvidenceMeasurement, LiveShadowError> {
    let mut measured = EvidenceMeasurement::default();
    for handle in &reply.artifact_handles {
        let evidence = service.evidence(&EvidenceCommand {
            handle: handle.clone(),
        })?;
        measured.bytes = measured.bytes.saturating_add(evidence.byte_len);
        let Some(text) = evidence.inline_text else {
            continue;
        };
        let parsed = match serde_json::from_str::<Value>(&text) {
            Ok(parsed) => parsed,
            Err(err) if evidence.kind == "execution-summary" => {
                return Err(LiveShadowError::MalformedSummary(err));
            }
            Err(_) => continue,
        };
        let Some(executors) = parsed.get("executors").and_then(Value::as_array) else {
            continue;
        };
        let Some(browser) = parsed.get("browser_programs").and_then(Value::as_array) else {
            continue;
        };
        measured.executors = Some(u64::try_from(executors.len()).unwrap_or(u64::MAX));
        measured.browser_programs = Some(u64::try_from(browser.len()).unwrap_or(u64::MAX));
    }
    Ok(measured)
}
