//! Extracted command-bus helper.

use super::access::*;
use super::runner_coverage::{
    artifact_is_fresh, normalize_coverage_paths, runner_artifact_candidates, set_record_error,
};

#[allow(clippy::too_many_arguments)]
pub(in crate::service) fn execution_summary(
    run: &RunId,
    change: &str,
    revision: &RevisionId,
    range: &RevisionRange,
    requested_scope: &str,
    effective_scope: &str,
    scope_reason: &str,
    evidence_policy: &str,
    outcome: &str,
    records: &[ExecutorRecord],
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
) -> Result<Vec<u8>, BusError> {
    let items: Vec<_> = records
        .iter()
        .map(|record| {
            json!({
                "executor": record.executor,
                "cwd": record.cwd,
                "selection": record.selection,
                "status_code": record.status_code,
                "passed": record.passed,
                "error": record.error,
                "stdout_bytes": record.stdout.len(),
                "stderr_bytes": record.stderr.len(),
                "artifacts": record.artifacts.iter().map(|artifact| json!({
                    "kind": artifact.kind,
                    "path": artifact.path,
                    "bytes": artifact.bytes.len(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let browser_items = browser_runs
        .iter()
        .map(|(configured, result)| {
            json!({
                "program": result.program,
                "path": configured.path,
                "passed": result.passed,
                "asserted": result.asserted,
                "contradicted": result.contradicted,
                "observations": result.observations.len(),
                "screenshots": result.screenshot_paths.len(),
                "trace": result.trace_path.is_some(),
                "failure": result.failure,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_vec_pretty(&json!({
        "schema_v": 1,
        "run_id": run.as_str(),
        "change": change,
        "revision": revision.as_str(),
        "base": {"ref": range.base_ref, "commit": range.base_commit},
        "head": {"ref": range.head_ref, "commit": range.head_commit},
        "merge_base": range.merge_base,
        "requested_scope": requested_scope,
        "effective_scope": effective_scope,
        "scope_reason": scope_reason,
        "evidence_policy": evidence_policy,
        "outcome": outcome,
        "executors": items,
        "browser_programs": browser_items,
    }))
    .map_err(|err| BusError::Runtime(format!("cannot encode execution summary: {err}")))
}

pub(in crate::service) const MAX_RUNNER_ARTIFACT_BYTES: u64 = 8 * 1024 * 1024;

pub(in crate::service) fn clear_generated_runner_artifacts(cwd: &Path) -> Result<(), BusError> {
    for relative in [
        ".weavatrix-quality/junit.xml",
        ".weavatrix-quality/go-cover.out",
    ] {
        let path = cwd.join(relative);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(BusError::Runtime(format!(
                    "cannot clear generated runner artifact {}: {err}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub(in crate::service) fn attach_normalized_artifacts(
    repo: &Path,
    cwd: &Path,
    started: SystemTime,
    record: &mut ExecutorRecord,
) {
    if record.executor == "cargo-test" && (!record.stdout.is_empty() || !record.stderr.is_empty()) {
        match std::str::from_utf8(&record.stdout)
            .map_err(|err| format!("cargo-test stdout is not UTF-8: {err}"))
            .and_then(|stdout| {
                std::str::from_utf8(&record.stderr)
                    .map_err(|err| format!("cargo-test stderr is not UTF-8: {err}"))
                    .map(|stderr| (stdout, stderr))
            })
            .and_then(|(stdout, stderr)| {
                parse_cargo_test(stdout, stderr).map_err(|err| err.to_string())
            })
            .and_then(|run| {
                serde_json::to_vec_pretty(&run)
                    .map_err(|err| format!("cannot encode normalized cargo-test: {err}"))
            }) {
            Ok(bytes) => record.artifacts.push(ProducedArtifact {
                kind: "normalized-test-run".into(),
                path: "cargo-test#normalized".into(),
                bytes,
            }),
            Err(err) => set_record_error(record, err),
        }
    }

    if record.executor == "go-test" && !record.stdout.is_empty() {
        match std::str::from_utf8(&record.stdout)
            .map_err(|err| format!("go-json output is not UTF-8: {err}"))
            .and_then(|text| parse_go_json(text).map_err(|err| err.to_string()))
            .and_then(|run| {
                serde_json::to_vec_pretty(&run)
                    .map_err(|err| format!("cannot encode normalized go-json: {err}"))
            }) {
            Ok(bytes) => record.artifacts.push(ProducedArtifact {
                kind: "normalized-test-run".into(),
                path: "stdout#normalized".into(),
                bytes,
            }),
            Err(err) => set_record_error(record, err),
        }
    }

    for (path, kind) in runner_artifact_candidates(cwd) {
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) | Err(_) => continue,
        };
        if !artifact_is_fresh(&metadata, started) {
            continue;
        }
        if metadata.len() > MAX_RUNNER_ARTIFACT_BYTES {
            set_record_error(
                record,
                format!(
                    "runner artifact {} exceeds {} bytes",
                    path.display(),
                    MAX_RUNNER_ARTIFACT_BYTES
                ),
            );
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                set_record_error(
                    record,
                    format!("cannot read runner artifact {}: {err}", path.display()),
                );
                continue;
            }
        };
        let display_path = relative_or_display(cwd, &path);
        record.artifacts.push(ProducedArtifact {
            kind: kind.into(),
            path: display_path.clone(),
            bytes: bytes.clone(),
        });

        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(err) => {
                set_record_error(
                    record,
                    format!("runner artifact {} is not UTF-8: {err}", path.display()),
                );
                continue;
            }
        };
        let normalized = match kind {
            "junit" => parse_junit(text)
                .and_then(|run| {
                    let failed_cases = run
                        .cases
                        .iter()
                        .filter(|case| matches!(case.status, TestStatus::Fail | TestStatus::Error))
                        .count();
                    serde_json::to_vec_pretty(&run)
                        .map_err(|err| wvq_runtime::RuntimeError::Malformed {
                            kind: "normalized-test-run".into(),
                            message: err.to_string(),
                        })
                        .map(|bytes| (bytes, failed_cases))
                })
                .map(|(bytes, failed_cases)| ("normalized-test-run", bytes, failed_cases)),
            "lcov" => parse_lcov(text)
                .map(|mut coverage| {
                    normalize_coverage_paths(repo, cwd, &mut coverage);
                    coverage
                })
                .and_then(|coverage| {
                    serde_json::to_vec_pretty(&coverage).map_err(|err| {
                        wvq_runtime::RuntimeError::Malformed {
                            kind: "coverage".into(),
                            message: err.to_string(),
                        }
                    })
                })
                .map(|bytes| ("coverage", bytes, 0)),
            "go-coverprofile" => parse_go_coverprofile(text)
                .map(|mut coverage| {
                    normalize_coverage_paths(repo, cwd, &mut coverage);
                    coverage
                })
                .and_then(|coverage| {
                    serde_json::to_vec_pretty(&coverage).map_err(|err| {
                        wvq_runtime::RuntimeError::Malformed {
                            kind: "coverage".into(),
                            message: err.to_string(),
                        }
                    })
                })
                .map(|bytes| ("coverage", bytes, 0)),
            _ => continue,
        };
        match normalized {
            Ok((normalized_kind, bytes, failed_cases)) => {
                if failed_cases > 0 {
                    set_record_error(
                        record,
                        format!(
                            "runner artifact {display_path} reports {failed_cases} failed or errored test case(s)"
                        ),
                    );
                }
                record.artifacts.push(ProducedArtifact {
                    kind: normalized_kind.into(),
                    path: format!("{display_path}#normalized"),
                    bytes,
                });
            }
            Err(err) => set_record_error(record, err.to_string()),
        }
    }
}

