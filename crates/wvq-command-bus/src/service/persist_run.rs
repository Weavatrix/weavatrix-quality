//! Extracted command-bus helper.

use super::access::*;

pub(in crate::service) fn make_run_id(change: &str, revision: &RevisionId) -> Result<RunId, BusError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| BusError::Identity(err.to_string()))?
        .as_nanos();
    let seed = format!(
        "{change}\0{}\0{nanos}\0{}",
        revision.as_str(),
        std::process::id()
    );
    let digest = sha256_hex(seed.as_bytes());
    RunId::new(format!("run-{}-{nanos}", &digest[..16]))
        .map_err(|err| BusError::Identity(err.to_string()))
}

pub(in crate::service) fn make_ai_usage_id(change: &str, kind: &str) -> Result<String, BusError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| BusError::Identity(err.to_string()))?
        .as_nanos();
    let seed = format!("{change}\0{kind}\0{nanos}\0{}", std::process::id());
    Ok(format!("ai-{}-{nanos}", &sha256_hex(seed.as_bytes())[..16]))
}

pub(in crate::service) fn put_run_artifact(
    store: &Store,
    run: &RunId,
    raw_id: &str,
    kind: &str,
    bytes: &[u8],
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let id = ArtifactId::new(raw_id).map_err(|err| BusError::Identity(err.to_string()))?;
    store
        .put_artifact(&id, kind, bytes)
        .map_err(|err| BusError::Store(err.to_string()))?;
    store
        .attach_run_artifact(run, &id)
        .map_err(|err| BusError::Store(err.to_string()))?;
    handles.push(id.to_string());
    Ok(())
}

pub(in crate::service) fn put_json_run_artifact<T: serde::Serialize>(
    store: &Store,
    run: &RunId,
    raw_id: &str,
    kind: &str,
    value: &T,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| BusError::Runtime(format!("cannot encode {kind}: {err}")))?;
    put_run_artifact(store, run, raw_id, kind, &bytes, handles)
}

pub(in crate::service) fn obligation_execution_map(
    repo: &Path,
    bindings: &[TestBinding],
    records: &[ExecutorRecord],
    run_id: &RunId,
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
    run_evidence_policy: &str,
) -> Result<StoredObligationExecutionMap, BusError> {
    let mut obligations = BTreeMap::<String, BTreeSet<StoredObligationExecution>>::new();
    for record in records {
        for artifact in record
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == "normalized-test-run")
        {
            let normalized: NormalizedTestRun =
                serde_json::from_slice(&artifact.bytes).map_err(|err| {
                    BusError::Runtime(format!(
                        "cannot decode normalized evidence from {}: {err}",
                        artifact.path
                    ))
                })?;
            for binding in bindings.iter().filter(|binding| {
                binding.case.is_some()
                    && binding
                        .runner
                        .as_deref()
                        .is_none_or(|runner| runner == record.executor)
            }) {
                for case in normalized.cases.iter().filter(|case| {
                    binding.case.as_deref() == Some(case.name.as_str())
                        && normalized_suite_matches(repo, record, binding, &case.suite)
                }) {
                    let evidence = StoredObligationExecution {
                        executor: record.executor.clone(),
                        path: binding.path.clone(),
                        suite: case.suite.clone(),
                        case: case.name.clone(),
                        status: normalized_status(case.status).into(),
                        invocation_passed: record.passed,
                        assertion: None,
                        observation: None,
                    };
                    for obligation in &binding.obligations {
                        obligations
                            .entry(obligation.clone())
                            .or_default()
                            .insert(evidence.clone());
                    }
                }
            }
        }
    }

    for (program_index, (configured, run)) in browser_runs.iter().enumerate() {
        for assertion in &run.assertions {
            let status = match assertion.status {
                BrowserAssertionStatus::Passed => "passed",
                BrowserAssertionStatus::Contradicted => "contradicted",
                BrowserAssertionStatus::Failed => "failed",
            };
            let observation = (run_evidence_policy != "none").then(|| {
                format!(
                    "artifact-{}-browser-{program_index}-observation-{}",
                    run_id.as_str(),
                    assertion.observation
                )
            });
            obligations
                .entry(assertion.obligation.clone())
                .or_default()
                .insert(StoredObligationExecution {
                    executor: "playwright-browser".into(),
                    path: configured.path.clone(),
                    suite: configured.path.clone(),
                    case: run.program.clone(),
                    status: status.into(),
                    invocation_passed: run.passed,
                    assertion: Some(format!("step:{}", assertion.step)),
                    observation,
                });
        }
    }

    Ok(StoredObligationExecutionMap {
        schema_v: 2,
        obligations: obligations
            .into_iter()
            .map(|(obligation, evidence)| (obligation, evidence.into_iter().collect()))
            .collect(),
    })
}

pub(in crate::service) fn normalized_suite_matches(
    repo: &Path,
    record: &ExecutorRecord,
    binding: &TestBinding,
    observed_suite: &str,
) -> bool {
    let expected = binding.suite.as_deref().unwrap_or(&binding.path);
    if normalize_path(observed_suite) == normalize_path(expected) {
        return true;
    }
    let observed = Path::new(observed_suite);
    let observed = if observed.is_absolute() {
        observed.to_path_buf()
    } else {
        repo.join(&record.cwd).join(observed)
    };
    let expected = repo.join(&binding.path);
    std::fs::canonicalize(observed)
        .ok()
        .zip(std::fs::canonicalize(expected).ok())
        .is_some_and(|(observed, expected)| observed == expected)
}

pub(in crate::service) fn normalized_status(status: TestStatus) -> &'static str {
    match status {
        TestStatus::Pass => "passed",
        TestStatus::Fail => "failed",
        TestStatus::Skip => "skipped",
        TestStatus::Error => "error",
    }
}

pub(in crate::service) fn severity_token(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "info",
        Severity::Warn => "warn",
        Severity::Error => "error",
    }
}
