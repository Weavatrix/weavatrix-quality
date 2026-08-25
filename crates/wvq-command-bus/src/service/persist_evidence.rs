//! Extracted command-bus helper.

use super::access::*;

pub(in crate::service) fn remove_browser_evidence_file(path: &Path) -> Result<(), BusError> {
    std::fs::remove_file(path).map_err(|err| {
        BusError::Runtime(format!(
            "cannot remove imported browser evidence {}: {err}",
            path.display()
        ))
    })
}

pub(in crate::service) fn capture_active(policy: CaptureWhen, failed: bool) -> bool {
    matches!(policy, CaptureWhen::Always) || (failed && matches!(policy, CaptureWhen::OnFailure))
}

pub(in crate::service) fn browser_capture_active(policy: CaptureWhen, failed: bool, run_policy: &str) -> bool {
    match run_policy {
        "standard" => capture_active(policy, failed),
        "minimal" => failed && !matches!(policy, CaptureWhen::Never),
        _ => false,
    }
}

pub(in crate::service) fn cap_browser_evidence(program: &mut TestProgram, run_policy: &str) {
    let cap = |capture: CaptureWhen| match run_policy {
        "minimal" if matches!(capture, CaptureWhen::Always) => CaptureWhen::OnFailure,
        "standard" | "minimal" => capture,
        _ => CaptureWhen::Never,
    };
    program.evidence_policy.screenshot = cap(program.evidence_policy.screenshot);
    program.evidence_policy.trace = cap(program.evidence_policy.trace);
    program.evidence_policy.network = cap(program.evidence_policy.network);
    program.evidence_policy.console = cap(program.evidence_policy.console);
    program.evidence_policy.storage = cap(program.evidence_policy.storage);
}


pub(in crate::service) fn parse_obligation_execution_map(
    bytes: &[u8],
) -> Result<BTreeMap<String, Vec<StoredObligationExecution>>, BusError> {
    let stored: StoredObligationExecutionMap = serde_json::from_slice(bytes)
        .map_err(|err| BusError::Store(format!("invalid obligation execution map: {err}")))?;
    if stored.schema_v != 2 {
        return Err(BusError::Store(
            "unknown obligation execution map schema".into(),
        ));
    }
    for (obligation, entries) in &stored.obligations {
        if obligation.is_empty() {
            return Err(BusError::Store(
                "obligation execution map has an empty obligation identity".into(),
            ));
        }
        for entry in entries {
            if entry.executor.is_empty()
                || entry.path.is_empty()
                || entry.suite.is_empty()
                || entry.case.is_empty()
                || !matches!(
                    entry.status.as_str(),
                    "passed" | "failed" | "skipped" | "error" | "contradicted"
                )
                || entry.assertion.as_deref().is_some_and(str::is_empty)
                || entry.observation.as_deref().is_some_and(str::is_empty)
            {
                return Err(BusError::Store(format!(
                    "obligation execution map {obligation} has invalid exact evidence"
                )));
            }
        }
    }
    Ok(stored.obligations)
}

pub(in crate::service) fn parse_revision_range_evidence(bytes: &[u8]) -> Result<RevisionRange, BusError> {
    let stored: StoredRevisionRangeEvidence = serde_json::from_slice(bytes)
        .map_err(|err| BusError::Store(format!("invalid revision-range evidence: {err}")))?;
    if stored.schema_v != 2
        || stored.base.reference.is_empty()
        || stored.head.reference.is_empty()
        || stored
            .head
            .content_revision
            .as_deref()
            .is_none_or(str::is_empty)
        || !valid_commit_id(&stored.base.commit)
        || !valid_commit_id(&stored.head.commit)
        || !valid_commit_id(&stored.merge_base)
    {
        return Err(BusError::Store(
            "revision-range evidence has invalid exact provenance".into(),
        ));
    }
    Ok(RevisionRange {
        base_ref: stored.base.reference,
        base_commit: stored.base.commit,
        head_ref: stored.head.reference,
        head_commit: stored.head.commit,
        head_content_revision: stored.head.content_revision.unwrap_or_default(),
        merge_base: stored.merge_base,
    })
}

pub(in crate::service) fn valid_commit_id(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(in crate::service) fn browser_evidence_kinds(
    configured: &ConfiguredBrowserProgram,
    result: &BrowserProgramRun,
    run_evidence_policy: &str,
) -> Vec<EvidenceKind> {
    let failed = !result.passed;
    let policy = &configured.program.evidence_policy;
    let mut present = Vec::new();
    if result
        .observations
        .iter()
        .any(|observation| observation.a11y_digest.is_some())
    {
        present.push(EvidenceKind::Dom);
    }
    if browser_capture_active(policy.network, failed, run_evidence_policy) {
        present.push(EvidenceKind::Network);
    }
    if browser_capture_active(policy.console, failed, run_evidence_policy) {
        present.push(EvidenceKind::Console);
    }
    if browser_capture_active(policy.storage, failed, run_evidence_policy)
        && result
            .observations
            .iter()
            .any(|observation| observation.storage_available)
    {
        present.push(EvidenceKind::Storage);
    }
    if run_evidence_policy != "none" && !result.screenshot_paths.is_empty() {
        present.push(EvidenceKind::Screenshot);
    }
    if run_evidence_policy != "none" && result.trace_path.is_some() {
        present.push(EvidenceKind::Trace);
    }
    present.sort_by_key(|kind| format!("{kind:?}"));
    present.dedup();
    present
}
