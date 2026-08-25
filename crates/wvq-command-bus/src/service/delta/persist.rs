//! Persist the live Delta Triangle artifact.

use serde_json::{Value, json};
use wvq_domain::RunId;
use wvq_proof::{FlowProtection, join_triangle, scoped_code_delta, scoped_spec_delta};
use wvq_runtime::{BrowserProgramRun, TestProgram};
use wvq_spec::diff_spec_scope;
use wvq_store::Store;

use super::super::{
    BaseBrowserReplay, BusError, ChangedFiles, Compiled, ConfiguredBrowserProgram,
    DELTA_TRIANGLE_KIND, put_json_run_artifact, severity_token,
};
use super::graph::{graph_diff_changed_nodes, paired_observation_delta};

#[allow(
    clippy::if_not_else,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
pub(in crate::service) fn persist_delta_triangle(
    store: &Store,
    run_id: &RunId,
    compiled: &Compiled,
    changed: &ChangedFiles,
    graph_diff: &Value,
    code_flows: &[FlowProtection],
    head_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
    base_replay: Result<BaseBrowserReplay, BusError>,
    run_evidence_policy: &str,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let openspec_path_changed = changed.changes_openspec_change(&compiled.change);
    let changed_nodes = graph_diff_changed_nodes(graph_diff)?;
    let mut spec_changed = false;
    let mut code_changed = false;
    let mut measured_programs = 0_u64;
    let mut changed_programs = Vec::new();
    let mut readings = Vec::new();
    let mut findings = Vec::new();
    let mut unmeasured_programs = Vec::new();
    let mut code_unmeasured_programs = Vec::new();
    let mut program_deltas = Vec::new();
    let mut base_revision = None;
    let mut replay_limitation = None;

    match base_replay {
        Err(error) => {
            replay_limitation = Some(error.to_string());
            unmeasured_programs.extend(
                head_runs
                    .iter()
                    .map(|(configured, _)| configured.program.id.to_string()),
            );
        }
        Ok(base) => {
            base_revision = Some(base.revision.to_string());
            let spec_scope = if openspec_path_changed {
                diff_spec_scope(base.spec.as_ref(), &compiled.spec)?
            } else {
                wvq_spec::SpecChangeScope::default()
            };
            if base.runs.len() != head_runs.len() {
                unmeasured_programs.extend(
                    head_runs
                        .iter()
                        .map(|(configured, _)| configured.program.id.to_string()),
                );
            } else {
                for (program_index, ((configured, head), base_run)) in
                    head_runs.iter().zip(&base.runs).enumerate()
                {
                    let program = configured.program.id.to_string();
                    let base_handles = persist_base_browser_observations(
                        store,
                        run_id,
                        program_index,
                        base_run,
                        run_evidence_policy != "none",
                        handles,
                    )?;
                    let head_handles = if run_evidence_policy == "none" {
                        Vec::new()
                    } else {
                        (0..head.observations.len())
                            .map(|index| {
                                format!(
                                    "artifact-{}-browser-{program_index}-observation-{index}",
                                    run_id.as_str()
                                )
                            })
                            .collect::<Vec<_>>()
                    };
                    if base_run.program != program
                        || head.program != program
                        || !browser_measurement_complete(base_run, &configured.program)
                        || !browser_measurement_complete(head, &configured.program)
                        || base_run.observations.len() != head.observations.len()
                    {
                        unmeasured_programs.push(program.clone());
                        program_deltas.push(json!({
                            "program": program,
                            "measured": false,
                            "base_passed": base_run.passed,
                            "head_passed": head.passed,
                            "base_observations": base_handles,
                            "head_observations": head_handles,
                        }));
                        continue;
                    }
                    let delta =
                        paired_observation_delta(&base_run.observations, &head.observations);
                    let program_spec = scoped_spec_delta(
                        &spec_scope,
                        &compiled.obligations,
                        &configured.program.obligations,
                    );
                    spec_changed |= program_spec.changed;
                    let program_code = scoped_code_delta(
                        &configured.program.obligations,
                        code_flows,
                        &changed_nodes,
                    );
                    if !program_code.measured {
                        code_unmeasured_programs.push(program.clone());
                    }
                    code_changed |= program_code.measured && program_code.changed;
                    let triangle = join_triangle(&program_spec, &program_code, &delta, &program);
                    measured_programs = measured_programs.saturating_add(1);
                    if delta.changed() {
                        changed_programs.push(program.clone());
                    }
                    readings.push(triangle.reading.as_str().to_owned());
                    for finding in &triangle.findings {
                        findings.push(json!({
                            "check": finding.check.as_str(),
                            "severity": severity_token(finding.severity),
                            "program": program,
                            "detail": finding.summary,
                        }));
                    }
                    program_deltas.push(json!({
                        "program": program,
                        "measured": true,
                        "reading": triangle.reading.as_str(),
                        "behavior_changed": delta.changed(),
                        "spec_authorized": program_spec.changed,
                        "authorized_obligations": program_spec.authorized_obligations,
                        "unauthorized_obligations": program_spec.unauthorized_obligations,
                        "code_measured": program_code.measured,
                        "code_changed": program_code.measured && program_code.changed,
                        "code_nodes": program_code.intersecting_nodes,
                        "code_unmeasured_reason": program_code.unmeasured_reason,
                        "first_behavior_axis": triangle.first_behavior_axis,
                        "visual_compared": triangle.visual_compared,
                        "pixel_compared": triangle.visual_compared,
                        "changed_axes": delta.axes.iter().map(|axis| axis.axis.as_str()).collect::<Vec<_>>(),
                        "base_observations": base_handles,
                        "head_observations": head_handles,
                    }));
                }
            }
        }
    }
    changed_programs.sort();
    changed_programs.dedup();
    unmeasured_programs.sort();
    unmeasured_programs.dedup();
    code_unmeasured_programs.sort();
    code_unmeasured_programs.dedup();
    readings.sort();
    let has_blocking = findings
        .iter()
        .any(|finding| finding.get("severity").and_then(Value::as_str) == Some("error"));
    let state = if has_blocking {
        "blocking"
    } else if !unmeasured_programs.is_empty() {
        "unmeasured"
    } else if findings.is_empty() {
        "clean"
    } else {
        "warnings"
    };
    let document = json!({
        "schema_v": 3,
        "state": state,
        "spec_changed": spec_changed,
        "code_changed": code_changed,
        "behavior_changed": !changed_programs.is_empty(),
        "measured_programs": measured_programs,
        "changed_programs": changed_programs,
        "readings": readings,
        "findings": findings,
        "unmeasured_programs": unmeasured_programs,
        "code_unmeasured_programs": code_unmeasured_programs,
        "base_revision": base_revision,
        "replay_limitation": replay_limitation,
        "programs": program_deltas,
        "runtime_llm_tokens": 0,
    });
    put_json_run_artifact(
        store,
        run_id,
        &format!("artifact-{}-delta-triangle", run_id.as_str()),
        DELTA_TRIANGLE_KIND,
        &document,
        handles,
    )
}

fn browser_measurement_complete(result: &BrowserProgramRun, program: &TestProgram) -> bool {
    let all_steps_observed = result.action_spans.len() == program.steps.len()
        && result.observations.len() == program.steps.len().saturating_add(1);
    all_steps_observed
        && (result.passed
            || result
                .failure
                .as_deref()
                .is_some_and(|failure| failure.starts_with("assertion_failed:")))
}

fn persist_base_browser_observations(
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
            "artifact-{}-delta-base-{program_index}-observation-{index}",
            run_id.as_str()
        );
        put_json_run_artifact(
            store,
            run_id,
            &id,
            "base-browser-observation",
            observation,
            handles,
        )?;
        observation_handles.push(id);
    }
    let evidence = json!({
        "schema_v": 1,
        "program": result.program,
        "passed": result.passed,
        "observations": observation_handles,
        "action_spans": result.action_spans,
    });
    put_json_run_artifact(
        store,
        run_id,
        &format!(
            "artifact-{}-delta-base-{program_index}-program",
            run_id.as_str()
        ),
        "base-browser-program-evidence",
        &evidence,
        handles,
    )?;
    Ok(observation_handles)
}
