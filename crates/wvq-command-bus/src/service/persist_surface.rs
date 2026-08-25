//! Persist the Application Surface Graph as a read-only run artifact.
//!
//! This is not a verdict axis. Missing coverage stays unmeasured.

use super::access::*;
use super::persist_run::put_json_run_artifact;
use super::selection_audit::read_single_run_json;
use super::verify_reply::artifact_handle_of_kind;
use crate::replies::ApplicationSurfaceView;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::service) struct ApplicationSurfaceDocument {
    pub(in crate::service) schema_v: u32,
    pub(in crate::service) revision: String,
    pub(in crate::service) truncated: bool,
    pub(in crate::service) surfaces: Vec<ApplicationSurfaceReading>,
    pub(in crate::service) protected: Vec<String>,
    pub(in crate::service) partial: Vec<String>,
    pub(in crate::service) unmeasured: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::service) struct ApplicationSurfaceReading {
    pub(in crate::service) id: String,
    pub(in crate::service) kind: ApplicationSurfaceKind,
    pub(in crate::service) state: SurfaceProjectionState,
    pub(in crate::service) covered_nodes: u64,
    pub(in crate::service) uncovered_nodes: u64,
    pub(in crate::service) unmeasured_nodes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::service) enum SurfaceProjectionState {
    Protected,
    Partial,
    Unmeasured,
}

pub(in crate::service) fn application_surface_document(
    revision: &RevisionId,
    graph: &Value,
    records: &[ExecutorRecord],
) -> Result<ApplicationSurfaceDocument, BusError> {
    let surface_graph = application_surface_graph(graph);
    let coverage = coverage_from_records(graph, records)?;
    let autopilot = coverage_autopilot(&surface_graph, &coverage);
    let mut protected = Vec::new();
    let mut partial = Vec::new();
    let mut unmeasured = Vec::new();
    let mut surfaces = Vec::new();
    for reading in &autopilot.surfaces {
        let state = project_state(reading.state);
        match state {
            SurfaceProjectionState::Protected => protected.push(reading.surface.clone()),
            SurfaceProjectionState::Partial => partial.push(reading.surface.clone()),
            SurfaceProjectionState::Unmeasured => unmeasured.push(reading.surface.clone()),
        }
        surfaces.push(ApplicationSurfaceReading {
            id: reading.surface.clone(),
            kind: reading.kind,
            state,
            covered_nodes: reading.covered_nodes,
            uncovered_nodes: reading.uncovered_nodes,
            unmeasured_nodes: reading.unmeasured_nodes,
        });
    }
    Ok(ApplicationSurfaceDocument {
        schema_v: 1,
        revision: revision.to_string(),
        truncated: autopilot.truncated,
        surfaces,
        protected,
        partial,
        unmeasured,
    })
}

pub(in crate::service) fn persist_application_surface_graph(
    store: &Store,
    run: &RunId,
    revision: &RevisionId,
    graph: &Value,
    records: &[ExecutorRecord],
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let document = application_surface_document(revision, graph, records)?;
    put_json_run_artifact(
        store,
        run,
        &format!("artifact-{}-application-surface-graph", run.as_str()),
        APPLICATION_SURFACE_GRAPH_KIND,
        &document,
        handles,
    )
}

pub(in crate::service) fn load_application_surface(
    store: &Store,
    run: &RunId,
) -> Result<ApplicationSurfaceView, BusError> {
    match read_single_run_json(store, run, APPLICATION_SURFACE_GRAPH_KIND) {
        Ok(value) => Ok(view_from_json(&value)?),
        Err(BusError::Store(message)) if message.contains("has no ") => {
            Ok(ApplicationSurfaceView::absent())
        }
        Err(err) => Err(err),
    }
}

pub(in crate::service) fn explain_application_surface(
    store: &Store,
    id: &str,
) -> Result<Option<ExplainReply>, BusError> {
    if !looks_like_surface(id) {
        return Ok(None);
    }
    let Some(run) = store
        .latest_run_any()
        .map_err(|err| BusError::Store(err.to_string()))?
    else {
        return Ok(None);
    };
    let Ok(document) = read_single_run_json(store, &run.id, APPLICATION_SURFACE_GRAPH_KIND) else {
        return Ok(None);
    };
    let parsed = parse_document(&document)?;
    let Some(surface) = parsed.surfaces.iter().find(|item| item.id == id) else {
        return Ok(None);
    };
    let mut provenance = vec![
        format!("surface {}", surface.id),
        format!("kind {}", surface.kind_token()),
        format!("state {}", surface.state_token()),
        format!("head revision {}", parsed.revision),
        format!(
            "nodes covered {} uncovered {} unmeasured {}",
            surface.covered_nodes, surface.uncovered_nodes, surface.unmeasured_nodes
        ),
    ];
    if parsed.truncated {
        provenance.push("projection truncated".into());
    }
    if let Some(handle) = artifact_handle_of_kind(store, &run.id, APPLICATION_SURFACE_GRAPH_KIND)? {
        provenance.push(format!("artifact {APPLICATION_SURFACE_GRAPH_KIND} {handle}"));
    }
    Ok(Some(ExplainReply {
        id: id.to_owned(),
        kind: "application_surface".into(),
        summary: format!("application surface {id} is {}", surface.state_token()),
        provenance,
    }))
}

fn view_from_json(value: &Value) -> Result<ApplicationSurfaceView, BusError> {
    let document = parse_document(value)?;
    Ok(ApplicationSurfaceView {
        present: true,
        truncated: document.truncated,
        protected: document.protected,
        partial: document.partial,
        unmeasured: document.unmeasured,
    })
}

fn parse_document(value: &Value) -> Result<ApplicationSurfaceDocument, BusError> {
    let schema_v = value.get("schema_v").and_then(Value::as_u64).ok_or_else(|| {
        BusError::Store("application-surface-graph omitted schema_v".into())
    })?;
    if schema_v != 1 {
        return Err(BusError::Store(format!(
            "unknown application-surface-graph schema version {schema_v}"
        )));
    }
    serde_json::from_value(value.clone()).map_err(|err| {
        BusError::Store(format!("malformed application-surface-graph: {err}"))
    })
}

pub(in crate::service) fn coverage_from_records(
    graph: &Value,
    records: &[ExecutorRecord],
) -> Result<Vec<NodeCoverage>, BusError> {
    let mut by_node = BTreeMap::<String, NodeCoverage>::new();
    let mut any = false;
    for record in records {
        for artifact in record
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == "coverage")
        {
            any = true;
            let coverage: CoverageArtifact =
                serde_json::from_slice(&artifact.bytes).map_err(|err| {
                    BusError::Runtime(format!(
                        "cannot decode normalized coverage {}: {err}",
                        artifact.path
                    ))
                })?;
            for node in map_coverage_to_nodes(Some(&coverage), graph)
                .map_err(|err| BusError::Intelligence(err.to_string()))?
            {
                match by_node.get_mut(&node.node_id) {
                    Some(existing) => {
                        existing.measurement =
                            stronger_measurement(existing.measurement, node.measurement);
                        existing.covered_lines = existing.covered_lines.max(node.covered_lines);
                        existing.instrumented_lines =
                            existing.instrumented_lines.max(node.instrumented_lines);
                    }
                    None => {
                        by_node.insert(node.node_id.clone(), node);
                    }
                }
            }
        }
    }
    if !any {
        return Ok(Vec::new());
    }
    Ok(by_node.into_values().collect())
}

fn stronger_measurement(left: CoverageMeasurement, right: CoverageMeasurement) -> CoverageMeasurement {
    match (left, right) {
        (CoverageMeasurement::Covered, _) | (_, CoverageMeasurement::Covered) => {
            CoverageMeasurement::Covered
        }
        (CoverageMeasurement::Uncovered, _) | (_, CoverageMeasurement::Uncovered) => {
            CoverageMeasurement::Uncovered
        }
        _ => CoverageMeasurement::Unmeasured,
    }
}

fn project_state(state: SurfaceCoverageState) -> SurfaceProjectionState {
    match state {
        SurfaceCoverageState::MeasuredCovered => SurfaceProjectionState::Protected,
        SurfaceCoverageState::MeasuredPartial => SurfaceProjectionState::Partial,
        SurfaceCoverageState::MeasuredUncovered | SurfaceCoverageState::Unmeasured => {
            SurfaceProjectionState::Unmeasured
        }
    }
}

fn looks_like_surface(id: &str) -> bool {
    id.starts_with("endpoint:")
        || id.starts_with("route:")
        || id.starts_with("component:")
        || id.starts_with("operation:")
        || id.starts_with("event:")
        || id.starts_with("public_api:")
}

impl ApplicationSurfaceReading {
    fn kind_token(&self) -> &'static str {
        match self.kind {
            ApplicationSurfaceKind::Route => "route",
            ApplicationSurfaceKind::Component => "component",
            ApplicationSurfaceKind::Endpoint => "endpoint",
            ApplicationSurfaceKind::Operation => "operation",
            ApplicationSurfaceKind::Event => "event",
            ApplicationSurfaceKind::PublicApi => "public_api",
        }
    }

    fn state_token(&self) -> &'static str {
        match self.state {
            SurfaceProjectionState::Protected => "protected",
            SurfaceProjectionState::Partial => "partial",
            SurfaceProjectionState::Unmeasured => "unmeasured",
        }
    }
}
