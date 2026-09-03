//! Persist the Surface Evidence Matrix as a read-only run artifact.
//!
//! Not a gate. Columns we cannot measure stay unmeasured.

use super::access::*;
use super::persist_run::put_json_run_artifact;
use super::persist_surface::coverage_from_records;
use super::selection_audit::read_single_run_json;
use crate::replies::SurfaceEvidenceMatrixView;
use serde::{Deserialize, Serialize};
use wvq_intelligence::{
    ApplicationSurfaceGraph, CoverageAutopilot, MeasuredColumn, SurfaceCoverageState,
    SurfaceEvidenceColumns, application_surface_graph, coverage_autopilot,
    production_nodes_for_binding, surface_evidence_matrix, surfaces_touching_nodes,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SurfaceEvidenceDocument {
    schema_v: u32,
    revision: String,
    truncated: bool,
    surfaces: Vec<wvq_intelligence::SurfaceEvidenceRow>,
}

pub(in crate::service) fn persist_surface_evidence_matrix(
    store: &Store,
    run: &RunId,
    revision: &RevisionId,
    graph: &Value,
    records: &[ExecutorRecord],
    bindings: &[TestBinding],
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let matrix = surface_evidence_document(graph, records, bindings)?;
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

pub(in crate::service) fn surface_evidence_document(
    graph: &Value,
    records: &[ExecutorRecord],
    bindings: &[TestBinding],
) -> Result<wvq_intelligence::SurfaceEvidenceMatrix, BusError> {
    let surface_graph = application_surface_graph(graph);
    let coverage = coverage_from_records(graph, records)?;
    let autopilot = coverage_autopilot(&surface_graph, &coverage);
    let (test, test_truncated) = binding_column(&surface_graph, graph, bindings);
    let (intent, intent_truncated) = intent_column(&surface_graph, graph, bindings);
    let columns = SurfaceEvidenceColumns {
        intent: Some(intent),
        test: Some(test),
        coverage: Some(coverage_column(&autopilot)),
        ..SurfaceEvidenceColumns::default()
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
    let schema_v = value.get("schema_v").and_then(Value::as_u64).ok_or_else(|| {
        BusError::Store("surface-evidence-matrix omitted schema_v".into())
    })?;
    let mut value = value.clone();
    if schema_v == 1 {
        migrate_v1_matrix(&mut value);
    } else if schema_v != 2 {
        return Err(BusError::Store(format!(
            "unknown surface-evidence-matrix schema version {schema_v}"
        )));
    }
    let document: SurfaceEvidenceDocument = serde_json::from_value(value).map_err(|err| {
        BusError::Store(format!("malformed surface-evidence-matrix: {err}"))
    })?;
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
