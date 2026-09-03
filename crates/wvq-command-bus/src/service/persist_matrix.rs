//! Persist the Surface Evidence Matrix as a read-only run artifact.
//!
//! Not a gate. Columns we cannot measure stay unmeasured.

use super::access::*;
use super::persist_run::put_json_run_artifact;
use super::persist_surface::coverage_from_records;
use super::selection_audit::read_single_run_json;
use crate::replies::SurfaceEvidenceMatrixView;
use crate::source_mutation::MutationRunDocument;
use serde::{Deserialize, Serialize};
use wvq_intelligence::{
    application_surface_graph, coverage_autopilot, production_nodes_for_binding,
    surface_evidence_matrix, surfaces_touching_nodes, ApplicationSurfaceGraph, CoverageAutopilot,
    MeasuredColumn, SurfaceCoverageState, SurfaceEvidenceColumns,
};
use wvq_proof::ProtectionSnapshot;

/// Live producers that can fill Surface Evidence Matrix columns.
pub(in crate::service) struct SurfaceEvidenceSources<'a> {
    pub graph: &'a Value,
    pub records: &'a [ExecutorRecord],
    pub bindings: &'a [TestBinding],
    pub mutation: Option<&'a MutationRunDocument>,
    pub browser_runs: &'a [(&'a ConfiguredBrowserProgram, BrowserProgramRun)],
    pub ui: Option<&'a UiIntegritySnapshot>,
    pub protection: Option<&'a ProtectionSnapshot>,
    /// `OBSERVED_ONLY` continuous journals. Fill Runtime, never Intent/Test/Proof.
    pub journals: &'a [ContinuousJournal],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SurfaceEvidenceDocument {
    schema_v: u32,
    revision: String,
    truncated: bool,
    surfaces: Vec<wvq_intelligence::SurfaceEvidenceRow>,
}

#[cfg(test)]
pub(in crate::service) fn persist_surface_evidence_matrix(
    store: &Store,
    run: &RunId,
    revision: &RevisionId,
    graph: &Value,
    records: &[ExecutorRecord],
    bindings: &[TestBinding],
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let matrix = surface_evidence_from(&SurfaceEvidenceSources {
        graph,
        records,
        bindings,
        mutation: None,
        browser_runs: &[],
        ui: None,
        protection: None,
        journals: &[],
    })?;
    let document = SurfaceEvidenceDocument {
        schema_v: 2,
        revision: revision.to_string(),
        truncated: matrix.truncated,
        surfaces: matrix.surfaces,
    };
    put_json_run_artifact(
        store,
        run,
        &format!("artifact-{}-surface-evidence-matrix", run.as_str()),
        SURFACE_EVIDENCE_MATRIX_KIND,
        &document,
        handles,
    )
}

#[cfg(test)]
pub(in crate::service) fn surface_evidence_document(
    graph: &Value,
    records: &[ExecutorRecord],
    bindings: &[TestBinding],
) -> Result<wvq_intelligence::SurfaceEvidenceMatrix, BusError> {
    surface_evidence_from(&SurfaceEvidenceSources {
        graph,
        records,
        bindings,
        mutation: None,
        browser_runs: &[],
        ui: None,
        protection: None,
        journals: &[],
    })
}

pub(in crate::service) fn persist_surface_evidence_from(
    store: &Store,
    run: &RunId,
    revision: &RevisionId,
    sources: &SurfaceEvidenceSources<'_>,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let matrix = surface_evidence_from(sources)?;
    let document = SurfaceEvidenceDocument {
        schema_v: 2,
        revision: revision.to_string(),
        truncated: matrix.truncated,
        surfaces: matrix.surfaces,
    };
    put_json_run_artifact(
        store,
        run,
        &format!("artifact-{}-surface-evidence-matrix", run.as_str()),
        SURFACE_EVIDENCE_MATRIX_KIND,
        &document,
        handles,
    )
}

pub(in crate::service) fn surface_evidence_from(
    sources: &SurfaceEvidenceSources<'_>,
) -> Result<wvq_intelligence::SurfaceEvidenceMatrix, BusError> {
    let surface_graph = application_surface_graph(sources.graph);
    let coverage = coverage_from_records(sources.graph, sources.records)?;
    let autopilot = coverage_autopilot(&surface_graph, &coverage);
    let (test, test_truncated) = binding_column(&surface_graph, sources.graph, sources.bindings);
    let (intent, intent_truncated) = intent_column(&surface_graph, sources.graph, sources.bindings);
    let columns = SurfaceEvidenceColumns {
        intent: Some(intent),
        test: Some(test),
        coverage: Some(coverage_column(&autopilot)),
        runtime: runtime_column(&surface_graph, sources.browser_runs, sources.journals),
        proof: proof_column(
            &surface_graph,
            sources.graph,
            sources.bindings,
            sources.records,
        ),
        protection: protection_column(&surface_graph, sources.protection),
        ui: ui_column(&surface_graph, sources.ui, false),
        a11y: ui_column(&surface_graph, sources.ui, true),
        mutation: mutation_column(&surface_graph, sources.graph, sources.mutation),
    };
    let mut matrix = surface_evidence_matrix(&surface_graph, &columns);
    matrix.truncated = matrix.truncated || test_truncated || intent_truncated;
    Ok(matrix)
}

pub(in crate::service) fn load_surface_evidence_matrix(
    store: &Store,
    run: &RunId,
) -> Result<SurfaceEvidenceMatrixView, BusError> {
    match read_single_run_json(store, run, SURFACE_EVIDENCE_MATRIX_KIND) {
        Ok(value) => Ok(view_from_json(&value)?),
        Err(BusError::Store(message)) if message.contains("has no ") => {
            Ok(SurfaceEvidenceMatrixView::absent())
        }
        Err(err) => Err(err),
    }
}

fn view_from_json(value: &Value) -> Result<SurfaceEvidenceMatrixView, BusError> {
    let schema_v = value
        .get("schema_v")
        .and_then(Value::as_u64)
        .ok_or_else(|| BusError::Store("surface-evidence-matrix omitted schema_v".into()))?;
    let mut value = value.clone();
    if schema_v == 1 {
        migrate_v1_matrix(&mut value);
    } else if schema_v != 2 {
        return Err(BusError::Store(format!(
            "unknown surface-evidence-matrix schema version {schema_v}"
        )));
    }
    let document: SurfaceEvidenceDocument = serde_json::from_value(value)
        .map_err(|err| BusError::Store(format!("malformed surface-evidence-matrix: {err}")))?;
    Ok(SurfaceEvidenceMatrixView {
        present: true,
        truncated: document.truncated,
        surfaces: document.surfaces,
    })
}

fn migrate_v1_matrix(value: &mut Value) {
    if let Some(surfaces) = value.get_mut("surfaces").and_then(Value::as_array_mut) {
        for row in surfaces {
            let Some(object) = row.as_object_mut() else {
                continue;
            };
            let old_protection = object
                .get("protection")
                .cloned()
                .unwrap_or_else(|| json!("unmeasured"));
            object.insert("coverage".into(), old_protection);
            object.insert("protection".into(), json!("unmeasured"));
        }
    }
    if let Some(object) = value.as_object_mut() {
        object.insert("schema_v".into(), json!(2));
    }
}

fn coverage_column(autopilot: &CoverageAutopilot) -> MeasuredColumn {
    let mut present = BTreeSet::new();
    let mut absent = BTreeSet::new();
    for row in &autopilot.surfaces {
        match row.state {
            SurfaceCoverageState::MeasuredCovered | SurfaceCoverageState::MeasuredPartial => {
                present.insert(row.surface.clone());
            }
            SurfaceCoverageState::MeasuredUncovered => {
                absent.insert(row.surface.clone());
            }
            SurfaceCoverageState::Unmeasured => {}
        }
    }
    MeasuredColumn { present, absent }
}

fn intent_column(
    surface_graph: &ApplicationSurfaceGraph,
    graph: &Value,
    bindings: &[TestBinding],
) -> (MeasuredColumn, bool) {
    let obligated = bindings
        .iter()
        .filter(|binding| !binding.obligations.is_empty())
        .cloned()
        .collect::<Vec<_>>();
    binding_column(surface_graph, graph, &obligated)
}

fn binding_column(
    surface_graph: &ApplicationSurfaceGraph,
    graph: &Value,
    bindings: &[TestBinding],
) -> (MeasuredColumn, bool) {
    let mut present = BTreeSet::new();
    let mut unknown = BTreeSet::new();
    let mut truncated = false;
    for binding in bindings {
        let reach = production_nodes_for_binding(graph, &binding.path);
        let touched = surfaces_touching_nodes(surface_graph, &reach.nodes);
        truncated = truncated || reach.truncated;
        if reach.truncated {
            unknown.extend(touched);
        } else {
            present.extend(touched);
        }
    }
    present.retain(|id| !unknown.contains(id));
    let absent = surface_graph
        .surfaces
        .iter()
        .map(|surface| surface.id.clone())
        .filter(|id| !present.contains(id) && !unknown.contains(id))
        .collect();
    (MeasuredColumn { present, absent }, truncated)
}

fn named_column(graph: &ApplicationSurfaceGraph, present: BTreeSet<String>) -> MeasuredColumn {
    MeasuredColumn::closed_world(graph, present)
}

fn surfaces_for_route(graph: &ApplicationSurfaceGraph, route: &str) -> BTreeSet<String> {
    let id = format!("route:{route}");
    graph
        .surfaces
        .iter()
        .filter(|surface| surface.id == id)
        .map(|surface| surface.id.clone())
        .collect()
}

fn surfaces_for_request(
    graph: &ApplicationSurfaceGraph,
    method: &str,
    url: &str,
) -> BTreeSet<String> {
    let path = request_path(url);
    let id = format!("endpoint:{} {path}", method.to_ascii_uppercase());
    graph
        .surfaces
        .iter()
        .filter(|surface| surface.id == id)
        .map(|surface| surface.id.clone())
        .collect()
}

fn request_path(url: &str) -> &str {
    let without_query = url.split('?').next().unwrap_or(url);
    if let Some(scheme) = without_query.find("://") {
        without_query[scheme + 3..]
            .find('/')
            .map_or("/", |index| &without_query[scheme + 3 + index..])
    } else {
        without_query
    }
}

pub(in crate::service) fn load_continuous_journals(
    store: &Store,
) -> Result<Vec<ContinuousJournal>, BusError> {
    let ids = store
        .artifact_ids_by_kind(super::CONTINUOUS_OBSERVATION_JOURNAL_KIND)
        .map_err(|err| BusError::Store(err.to_string()))?;
    let mut journals = Vec::new();
    for id in ids {
        let (_, bytes) = store
            .read_artifact(&id)
            .map_err(|err| BusError::Store(err.to_string()))?;
        let raw = String::from_utf8(bytes).map_err(|err| {
            BusError::Store(format!("continuous journal {id} is not UTF-8: {err}"))
        })?;
        journals.push(ContinuousJournal::from_json(&raw).map_err(|err| {
            BusError::Store(format!("continuous journal {id} is malformed: {err}"))
        })?);
    }
    Ok(journals)
}

fn runtime_column(
    graph: &ApplicationSurfaceGraph,
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
    journals: &[ContinuousJournal],
) -> Option<MeasuredColumn> {
    if browser_runs.is_empty() && journals.is_empty() {
        return None;
    }
    let mut present = BTreeSet::new();
    for (_, run) in browser_runs {
        for observation in &run.observations {
            if let Some(route) = &observation.route {
                present.extend(surfaces_for_route(graph, route));
            }
            for request in &observation.network_requests {
                present.extend(surfaces_for_request(graph, &request.method, &request.url));
            }
        }
    }
    for journal in journals {
        present.extend(surfaces_for_state(graph, &journal.initial));
        for event in &journal.events {
            present.extend(surfaces_for_state(graph, &event.after));
            present.extend(surfaces_for_action(graph, &event.action));
        }
    }
    Some(named_column(graph, present))
}

fn surfaces_for_state(graph: &ApplicationSurfaceGraph, state: &BehaviorState) -> BTreeSet<String> {
    let mut present = surfaces_for_route(graph, &state.route);
    if let Some(component) = &state.component {
        let id = format!("component:{component}");
        present.extend(
            graph
                .surfaces
                .iter()
                .filter(|surface| surface.id == id)
                .map(|surface| surface.id.clone()),
        );
    }
    present
}

fn surfaces_for_action(graph: &ApplicationSurfaceGraph, action: &TestAction) -> BTreeSet<String> {
    match action {
        TestAction::Navigate { route } => surfaces_for_route(graph, route),
        TestAction::ApiCall { operation, .. } => {
            let mut present = BTreeSet::new();
            for id in [
                format!("endpoint:{operation}"),
                format!("operation:{operation}"),
            ] {
                present.extend(
                    graph
                        .surfaces
                        .iter()
                        .filter(|surface| surface.id == id)
                        .map(|surface| surface.id.clone()),
                );
            }
            if let Some((method, path)) = operation.split_once(' ') {
                present.extend(surfaces_for_request(graph, method, path));
            }
            present
        }
        _ => BTreeSet::new(),
    }
}

fn proof_column(
    graph: &ApplicationSurfaceGraph,
    weavatrix: &Value,
    bindings: &[TestBinding],
    records: &[ExecutorRecord],
) -> Option<MeasuredColumn> {
    let mut present = BTreeSet::new();
    let mut saw_exact = false;
    for record in records.iter().filter(|record| record.passed) {
        for artifact in record
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == "normalized-test-run")
        {
            let Ok(normalized) = serde_json::from_slice::<NormalizedTestRun>(&artifact.bytes)
            else {
                continue;
            };
            for binding in bindings {
                let Some(case) = binding.case.as_deref() else {
                    continue;
                };
                if binding.obligations.is_empty() {
                    continue;
                }
                if !normalized
                    .cases
                    .iter()
                    .any(|item| item.name == case && item.status == wvq_runtime::TestStatus::Pass)
                {
                    continue;
                }
                saw_exact = true;
                let reach = production_nodes_for_binding(weavatrix, &binding.path);
                if !reach.truncated {
                    present.extend(surfaces_touching_nodes(graph, &reach.nodes));
                }
            }
        }
    }
    saw_exact.then(|| named_column(graph, present))
}

fn protection_column(
    graph: &ApplicationSurfaceGraph,
    snapshot: Option<&ProtectionSnapshot>,
) -> Option<MeasuredColumn> {
    let snapshot = snapshot?;
    let mut nodes = Vec::new();
    for flow in &snapshot.flows {
        nodes.push(flow.flow.clone());
        nodes.extend(flow.covered_nodes.iter().cloned());
    }
    Some(named_column(graph, surfaces_touching_nodes(graph, &nodes)))
}

fn ui_column(
    graph: &ApplicationSurfaceGraph,
    snapshot: Option<&UiIntegritySnapshot>,
    a11y: bool,
) -> Option<MeasuredColumn> {
    let snapshot = snapshot?;
    let mut present = BTreeSet::new();
    for finding in &snapshot.findings {
        if a11y == finding.check.is_a11y() {
            present.extend(surfaces_for_route(graph, &finding.route));
        }
    }
    for key in &snapshot.measured_states {
        if let Some(route) = route_from_state_key(key.as_str()) {
            present.extend(surfaces_for_route(graph, route));
        }
    }
    Some(named_column(graph, present))
}

fn route_from_state_key(key: &str) -> Option<&str> {
    let without_viewport = key.rsplit_once('@').map_or(key, |(state, _)| state);
    without_viewport.rsplit_once('@').map(|(_, route)| route)
}

fn mutation_column(
    graph: &ApplicationSurfaceGraph,
    weavatrix: &Value,
    mutation: Option<&MutationRunDocument>,
) -> Option<MeasuredColumn> {
    let mutation = mutation?;
    if mutation.state == "not_applicable" {
        return None;
    }
    let mut present = BTreeSet::new();
    for result in &mutation.results {
        let reach = production_nodes_for_binding(weavatrix, &result.path);
        if !reach.truncated {
            present.extend(surfaces_touching_nodes(graph, &reach.nodes));
        }
    }
    Some(named_column(graph, present))
}
