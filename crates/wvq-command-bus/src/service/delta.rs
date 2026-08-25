//! Per-program Spec × Code × Behavior persistence. Not a quality percentage.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use wvq_domain::RunId;
use wvq_proof::{FlowProtection, join_triangle, scoped_code_delta, scoped_spec_delta};
use wvq_runtime::{
    AxisDelta, BehaviorDelta, BrowserProgramRun, DiffAxis, StructuredView, TestProgram,
    behavior_delta,
};
use wvq_spec::diff_spec_scope;
use wvq_store::Store;

use super::{
    BaseBrowserReplay, BusError, ChangedFiles, Compiled, ConfiguredBrowserProgram, TestBinding,
    DELTA_TRIANGLE_KIND, ensure_complete_diff, graph_node_id, graph_node_source_path,
    normalize_path, put_json_run_artifact, severity_token, sha256_hex, values_at,
};

#[allow(
    clippy::if_not_else,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
pub(super) fn persist_delta_triangle(
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
                    if program_code.measured {
                        readings.push(triangle.reading.as_str().to_owned());
                    }
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
                        "reading": program_code.measured.then_some(triangle.reading.as_str()),
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
    } else if !unmeasured_programs.is_empty() || !code_unmeasured_programs.is_empty() {
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

fn paired_observation_delta(
    base: &[wvq_runtime::Observation],
    head: &[wvq_runtime::Observation],
) -> BehaviorDelta {
    let mut axes = BTreeMap::<DiffAxis, (Vec<String>, Vec<String>)>::new();
    let mut first_structured = None;
    for (index, (base, head)) in base.iter().zip(head).enumerate() {
        let delta = behavior_delta(
            &StructuredView::from_replay(base, None),
            &StructuredView::from_replay(head, None),
        );
        first_structured = first_structured.or(delta.first_structured);
        for axis in delta.axes {
            let entry = axes.entry(axis.axis).or_default();
            entry
                .0
                .push(format!("{index}:{}", sha256_hex(axis.base.as_bytes())));
            entry
                .1
                .push(format!("{index}:{}", sha256_hex(axis.head.as_bytes())));
        }
    }
    if first_structured.is_some() {
        axes.remove(&DiffAxis::VisualDigest);
    }
    let visual_compared = first_structured.is_none()
        && base.iter().zip(head).any(|(base_obs, head_obs)| {
            base_obs.visual_digest.is_some() && head_obs.visual_digest.is_some()
        });
    BehaviorDelta {
        axes: axes
            .into_iter()
            .map(|(axis, (base, head))| AxisDelta {
                axis,
                base: base.join(","),
                head: head.join(","),
            })
            .collect(),
        first_structured,
        visual_compared,
    }
}

pub(super) fn graph_diff_changed_nodes(diff: &Value) -> Result<BTreeSet<String>, BusError> {
    ensure_complete_diff(diff)?;
    let mut nodes = BTreeSet::new();
    for node in values_at(diff, "/nodes/added") {
        if let Some(id) = graph_node_id(node) {
            nodes.insert(id);
        }
    }
    for node in values_at(diff, "/nodes/removed") {
        if let Some(id) = graph_node_id(node) {
            nodes.insert(id);
        }
    }
    for changed in values_at(diff, "/nodes/changed") {
        if let Some(id) = changed.get("before").and_then(graph_node_id) {
            nodes.insert(id);
        }
        if let Some(id) = changed.get("after").and_then(graph_node_id) {
            nodes.insert(id);
        }
        if let Some(id) = graph_node_id(changed) {
            nodes.insert(id);
        }
    }
    for edge in values_at(diff, "/edges/added")
        .iter()
        .chain(values_at(diff, "/edges/removed"))
    {
        if let Some(source) = edge.get("source").and_then(Value::as_str)
            && !source.is_empty()
        {
            nodes.insert(source.to_owned());
        }
        if let Some(target) = edge.get("target").and_then(Value::as_str)
            && !target.is_empty()
        {
            nodes.insert(target.to_owned());
        }
    }
    Ok(nodes)
}

pub(super) fn declared_code_flows(
    revision: &str,
    bindings: &[TestBinding],
    graph: &Value,
) -> Vec<FlowProtection> {
    let mut by_path = BTreeMap::<String, BTreeSet<String>>::new();
    for binding in bindings {
        by_path
            .entry(normalize_path(&binding.path))
            .or_default()
            .extend(binding.obligations.iter().cloned());
    }
    let mut flows = BTreeMap::<String, FlowProtection>::new();
    for node in values_at(graph, "/nodes") {
        let Some(file) = graph_node_file(node) else {
            continue;
        };
        let Some(obligations) = by_path.get(&file) else {
            continue;
        };
        let Some(node_id) = graph_node_id(node) else {
            continue;
        };
        let flow = flows
            .entry(node_id.clone())
            .or_insert_with(|| FlowProtection {
                flow: node_id.clone(),
                revision: revision.to_owned(),
                tests: Vec::new(),
                sessions: Vec::new(),
                covered_nodes: vec![node_id.clone()],
                covered_branches: Vec::new(),
                proven_obligations: Vec::new(),
                proofs: Vec::new(),
            });
        for obligation in obligations {
            if !flow.proven_obligations.contains(obligation) {
                flow.proven_obligations.push(obligation.clone());
            }
        }
        for binding in bindings
            .iter()
            .filter(|binding| normalize_path(&binding.path) == file)
        {
            if !flow.tests.contains(&binding.path) {
                flow.tests.push(binding.path.clone());
            }
        }
    }
    let mut flows = flows.into_values().collect::<Vec<_>>();
    for flow in &mut flows {
        flow.proven_obligations.sort();
        flow.proven_obligations.dedup();
        flow.tests.sort();
        flow.tests.dedup();
    }
    flows
}

fn graph_node_file(node: &Value) -> Option<String> {
    node.pointer("/span/file")
        .and_then(Value::as_str)
        .or_else(|| graph_node_source_path(node))
        .map(normalize_path)
}
