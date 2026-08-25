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
        schema_v: 1,
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
    let (intent_test, reach_truncated) = binding_column(&surface_graph, graph, bindings);
    let columns = SurfaceEvidenceColumns {
        intent: Some(intent_test.clone()),
        test: Some(intent_test),
        protection: Some(protection_column(&autopilot)),
        ..SurfaceEvidenceColumns::default()
    };
    let mut matrix = surface_evidence_matrix(&surface_graph, &columns);
    matrix.truncated = matrix.truncated || reach_truncated;
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
    if schema_v != 1 {
        return Err(BusError::Store(format!(
            "unknown surface-evidence-matrix schema version {schema_v}"
        )));
    }
    let document: SurfaceEvidenceDocument = serde_json::from_value(value.clone()).map_err(|err| {
        BusError::Store(format!("malformed surface-evidence-matrix: {err}"))
    })?;
    Ok(SurfaceEvidenceMatrixView {
        present: true,
        truncated: document.truncated,
        surfaces: document.surfaces,
    })
}

fn protection_column(autopilot: &CoverageAutopilot) -> MeasuredColumn {
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
