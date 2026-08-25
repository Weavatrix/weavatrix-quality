//! Extracted command-bus helper.

use super::access::*;
use super::protection_graph_extra::{
    ObservedTestCase, PersistedTestAnalytics, TestAnalyticsDocument, TestOutcomeCounts,
};

pub(in crate::service) fn collect_observed_test_cases(
    records: &[ExecutorRecord],
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
) -> Result<Vec<ObservedTestCase>, BusError> {
    let mut observed = Vec::new();
    for record in records {
        for artifact in record
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == "normalized-test-run")
        {
            let normalized: NormalizedTestRun =
                serde_json::from_slice(&artifact.bytes).map_err(|err| {
                    BusError::Runtime(format!(
                        "cannot decode {} from {}: {err}",
                        artifact.kind, artifact.path
                    ))
                })?;
            observed.extend(normalized.cases.into_iter().map(|case| ObservedTestCase {
                executor: record.executor.clone(),
                suite: case.suite,
                name: case.name,
                status: case.status,
                duration_ms: case.duration_ms,
                message: case.message,
            }));
        }
        if let Some(error) = &record.error {
            observed.push(ObservedTestCase {
                executor: record.executor.clone(),
                suite: record.cwd.clone(),
                name: "<executor invocation>".into(),
                status: TestStatus::Error,
                duration_ms: None,
                message: Some(error.clone()),
            });
        }
    }
    observed.extend(
        browser_runs
            .iter()
            .map(|(configured, result)| ObservedTestCase {
                executor: "playwright-browser".into(),
                suite: configured.path.clone(),
                name: result.program.clone(),
                status: if result.passed {
                    TestStatus::Pass
                } else {
                    TestStatus::Fail
                },
                duration_ms: None,
                message: result.failure.clone(),
            }),
    );
    Ok(observed)
}

pub(in crate::service) fn persist_failure(
    store: &Store,
    run: &RunId,
    index: usize,
    case: &ObservedTestCase,
    status: &str,
) -> Result<(wvq_domain::ContentHash, FlakeClass, u64), BusError> {
    let message = case.message.as_deref().unwrap_or(status);
    let evidence = FailureEvidence {
        program: format!("{}::{}", case.suite, case.name),
        executor: case.executor.clone(),
        stack_digest: Some(sha256_hex(message.as_bytes())),
        timing_bucket: failure_timing_bucket(message),
        ..FailureEvidence::default()
    };
    let digest = fingerprint_id(&evidence).map_err(|err| BusError::Runtime(err.to_string()))?;
    let previous = store
        .failure_cluster_size(&digest)
        .map_err(|err| BusError::Store(err.to_string()))?;
    let classification = triage(&evidence, previous > 0).class;
    store
        .put_failure_fingerprint(&digest, flake_class_token(classification))
        .map_err(|err| BusError::Store(err.to_string()))?;
    store
        .put_failure_occurrence(&format!("{}-failure-{index}", run.as_str()), &digest)
        .map_err(|err| BusError::Store(err.to_string()))?;
    Ok((digest, classification, previous))
}

#[allow(clippy::too_many_lines)]
pub(in crate::service) fn persist_test_analytics(
    store: &Store,
    run: &RunId,
    revision: &RevisionId,
    records: &[ExecutorRecord],
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
) -> Result<PersistedTestAnalytics, BusError> {
    let observed = collect_observed_test_cases(records, browser_runs)?;

    let mut passed = 0_u64;
    let mut failed = 0_u64;
    let mut errors = 0_u64;
    let mut skipped = 0_u64;
    let mut unknown_failures = 0_u64;
    let mut failures = Vec::new();
    let mut flaky = BTreeMap::<(String, String, String), Value>::new();
    let mut durations = Vec::<(u64, Value)>::new();

    for (index, case) in observed.iter().enumerate() {
        match case.status {
            TestStatus::Pass => passed = passed.saturating_add(1),
            TestStatus::Fail => failed = failed.saturating_add(1),
            TestStatus::Error => errors = errors.saturating_add(1),
            TestStatus::Skip => skipped = skipped.saturating_add(1),
        }
        let status = test_status_token(case.status);
        let failure = if matches!(case.status, TestStatus::Fail | TestStatus::Error) {
            let (digest, classification, previous) =
                persist_failure(store, run, index, case, status)?;
            if classification == FlakeClass::Unknown {
                unknown_failures = unknown_failures.saturating_add(1);
            }
            failures.push(json!({
                "executor": case.executor,
                "suite": case.suite,
                "name": case.name,
                "status": status,
                "fingerprint": digest.as_str(),
                "classification": flake_class_token(classification),
                "previous_occurrences": previous,
            }));
            Some(digest)
        } else {
            None
        };
        store
            .put_test_case_result(&StoredTestCaseResult {
                id: format!("{}-test-{index}", run.as_str()),
                run_id: run.clone(),
                revision: revision.clone(),
                executor: case.executor.clone(),
                suite: case.suite.clone(),
                name: case.name.clone(),
                status: status.into(),
                duration_ms: case.duration_ms,
                fingerprint: failure,
            })
            .map_err(|err| BusError::Store(err.to_string()))?;
        let history = store
            .test_case_stats(&case.executor, &case.suite, &case.name)
            .map_err(|err| BusError::Store(err.to_string()))?;
        let identity = (case.executor.clone(), case.suite.clone(), case.name.clone());
        if history.flaky {
            flaky.insert(
                identity.clone(),
                json!({
                    "executor": case.executor,
                    "suite": case.suite,
                    "name": case.name,
                    "runs": history.runs,
                    "passes": history.passes,
                    "failures": history.failures,
                    "errors": history.errors,
                }),
            );
        }
        if let Some(duration_ms) = case.duration_ms {
            durations.push((
                duration_ms,
                json!({
                    "executor": case.executor,
                    "suite": case.suite,
                    "name": case.name,
                    "duration_ms": duration_ms,
                    "historical_average_ms": history.average_duration_ms,
                }),
            ));
        }
    }
    durations.sort_by(|left, right| right.0.cmp(&left.0));
    durations.truncate(20);
    let recorded = u64::try_from(observed.len()).unwrap_or(u64::MAX);
    let flaky_values = flaky.into_values().collect::<Vec<_>>();
    let flaky_count = u64::try_from(flaky_values.len()).unwrap_or(u64::MAX);
    let bytes = serde_json::to_vec_pretty(&TestAnalyticsDocument {
        schema_v: 1,
        run_id: run.to_string(),
        revision: revision.to_string(),
        recorded_cases: recorded,
        outcomes: TestOutcomeCounts {
            passed,
            failed,
            errors,
            skipped,
        },
        failure_occurrences: failures,
        flaky_tests: flaky_values,
        slowest_tests: durations.into_iter().map(|(_, value)| value).collect(),
        runtime_llm_tokens: 0,
    })
    .map_err(|err| BusError::Runtime(format!("cannot encode test analytics: {err}")))?;
    Ok(PersistedTestAnalytics {
        recorded_test_count: recorded,
        failed_test_count: failed.saturating_add(errors),
        flaky_test_count: flaky_count,
        unknown_failure_count: unknown_failures,
        bytes,
    })
}

pub(in crate::service) fn test_status_token(status: TestStatus) -> &'static str {
    match status {
        TestStatus::Pass => "pass",
        TestStatus::Fail => "fail",
        TestStatus::Skip => "skip",
        TestStatus::Error => "error",
    }
}

pub(in crate::service) fn failure_timing_bucket(message: &str) -> Option<TimingBucket> {
    let message = message.to_ascii_lowercase();
    (message.contains("timed out")
        || message.contains("timeout")
        || message.contains("deadline exceeded"))
    .then_some(TimingBucket::Timeout)
}

pub(in crate::service) fn flake_class_token(class: FlakeClass) -> &'static str {
    match class {
        FlakeClass::Known => "known",
        FlakeClass::ProductRegression => "product_regression",
        FlakeClass::Ordering => "ordering",
        FlakeClass::Timing => "timing",
        FlakeClass::Network => "network",
        FlakeClass::Environment => "environment",
        FlakeClass::SelectorDrift => "selector_drift",
        FlakeClass::Seed => "seed",
        FlakeClass::TestOrder => "test_order",
        FlakeClass::Unknown => "unknown",
    }
}

