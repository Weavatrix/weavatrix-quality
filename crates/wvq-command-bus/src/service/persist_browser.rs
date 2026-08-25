//! Extracted command-bus helper.

use super::access::*;
use super::persist_evidence::{browser_evidence_kinds, remove_browser_evidence_file};
use super::persist_run::{put_json_run_artifact, put_run_artifact};

pub(in crate::service) fn persist_browser_runs(
    store: &Store,
    run_id: &RunId,
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
    run_evidence_policy: &str,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let keep_normalized = run_evidence_policy != "none";
    for (program_index, (configured, result)) in browser_runs.iter().enumerate() {
        persist_browser_run(
            store,
            run_id,
            program_index,
            configured,
            result,
            run_evidence_policy,
            keep_normalized,
            handles,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(in crate::service) fn persist_browser_run(
    store: &Store,
    run_id: &RunId,
    program_index: usize,
    configured: &ConfiguredBrowserProgram,
    result: &BrowserProgramRun,
    run_evidence_policy: &str,
    keep_normalized: bool,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let token = safe_file_token(configured.program.id.as_str());
    let observation_handles = persist_browser_observations(
        store,
        run_id,
        program_index,
        result,
        keep_normalized,
        handles,
    )?;
    persist_browser_files(
        store,
        run_id,
        program_index,
        result,
        keep_normalized,
        handles,
    )?;
    if keep_normalized {
        if let Some(profile) = &result.network_profile {
            put_json_run_artifact(
                store,
                run_id,
                &format!(
                    "artifact-{}-browser-{program_index}-{token}-network-profile",
                    run_id.as_str()
                ),
                "network-replay-profile",
                profile,
                handles,
            )?;
        }
        if !result.network_limitations.is_empty() {
            put_json_run_artifact(
                store,
                run_id,
                &format!(
                    "artifact-{}-browser-{program_index}-{token}-network-limitations",
                    run_id.as_str()
                ),
                "network-replay-limitations",
                &json!({
                    "schema_v": 1,
                    "program": configured.program.id,
                    "limitations": result.network_limitations,
                }),
                handles,
            )?;
        }
    }
    let assertions = stored_browser_assertions(result, keep_normalized, &observation_handles)?;
    let evidence = StoredBrowserProgramEvidence {
        schema_v: 2,
        program: configured.program.id.to_string(),
        asserted: result.asserted.clone(),
        contradicted: result.contradicted.clone(),
        assertions,
        present: browser_evidence_kinds(configured, result, run_evidence_policy),
        observations: observation_handles,
    };
    put_json_run_artifact(
        store,
        run_id,
        &format!(
            "artifact-{}-browser-{program_index}-{token}",
            run_id.as_str()
        ),
        "browser-program-evidence",
        &evidence,
        handles,
    )
}

pub(in crate::service) fn persist_browser_observations(
    store: &Store,
    run_id: &RunId,
    program_index: usize,
    result: &BrowserProgramRun,
    keep: bool,
    handles: &mut Vec<String>,
) -> Result<Vec<String>, BusError> {
    let mut observation_handles = Vec::new();
    if !keep {
        return Ok(observation_handles);
    }
    for (index, observation) in result.observations.iter().enumerate() {
        let id = format!(
            "artifact-{}-browser-{program_index}-observation-{index}",
            run_id.as_str()
        );
        put_json_run_artifact(
            store,
            run_id,
            &id,
            "browser-observation",
            observation,
            handles,
        )?;
        observation_handles.push(id);
    }
    Ok(observation_handles)
}

pub(in crate::service) fn persist_browser_files(
    store: &Store,
    run_id: &RunId,
    program_index: usize,
    result: &BrowserProgramRun,
    keep: bool,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    for (index, path) in result.screenshot_paths.iter().enumerate() {
        if keep {
            let bytes = std::fs::read(path).map_err(|err| {
                BusError::Runtime(format!(
                    "cannot import browser screenshot {}: {err}",
                    path.display()
                ))
            })?;
            put_run_artifact(
                store,
                run_id,
                &format!(
                    "artifact-{}-browser-{program_index}-screenshot-{index}",
                    run_id.as_str()
                ),
                "screenshot",
                &bytes,
                handles,
            )?;
        }
        remove_browser_evidence_file(path)?;
    }
    if let Some(path) = &result.trace_path {
        if keep {
            let bytes = std::fs::read(path).map_err(|err| {
                BusError::Runtime(format!(
                    "cannot import browser trace {}: {err}",
                    path.display()
                ))
            })?;
            put_run_artifact(
                store,
                run_id,
                &format!("artifact-{}-browser-{program_index}-trace", run_id.as_str()),
                "playwright-trace",
                &bytes,
                handles,
            )?;
        }
        remove_browser_evidence_file(path)?;
    }
    Ok(())
}

pub(in crate::service) fn stored_browser_assertions(
    result: &BrowserProgramRun,
    keep: bool,
    observation_handles: &[String],
) -> Result<Vec<StoredBrowserAssertionEvidence>, BusError> {
    result
        .assertions
        .iter()
        .map(|assertion| {
            let status = match assertion.status {
                BrowserAssertionStatus::Passed => "passed",
                BrowserAssertionStatus::Contradicted => "contradicted",
                BrowserAssertionStatus::Failed => "failed",
            };
            let observation = if keep {
                Some(
                    observation_handles
                        .get(assertion.observation)
                        .cloned()
                        .ok_or_else(|| {
                            BusError::Runtime(format!(
                                "browser assertion step {} references missing observation {}",
                                assertion.step, assertion.observation
                            ))
                        })?,
                )
            } else {
                None
            };
            Ok(StoredBrowserAssertionEvidence {
                obligation: assertion.obligation.clone(),
                step: assertion.step,
                status: status.into(),
                observation,
            })
        })
        .collect()
}

pub(in crate::service) const BEHAVIOR_SAMPLE_LIMIT: usize = 500;
pub(in crate::service) const BEHAVIOR_PROGRAM_SAMPLE_LIMIT: usize = 100;

/// Largest UI evidence document written for one run.
pub(in crate::service) const MAX_UI_ARTIFACT_BYTES: usize = 4 * 1024 * 1024;

/// How many findings of one class reach the bounded verdict projection.
///
/// The full list stays in the CAS artifact; the reply carries enough to act on
/// without paying for hundreds of near-identical entries.
pub(in crate::service) const MAX_UI_REPLY_FINDINGS: usize = 25;

