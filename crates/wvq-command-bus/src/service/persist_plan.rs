//! Persist the cheapest-evidence plan as a read-only run artifact.
//!
//! Not a gate. Does not generate tests, previews, or model calls.

use super::access::*;
use super::persist_matrix::{surface_evidence_from, SurfaceEvidenceSources};
#[cfg(test)]
use super::persist_matrix::surface_evidence_document;
use super::persist_run::put_json_run_artifact;
use super::selection_audit::read_single_run_json;
use crate::replies::CheapestEvidencePlanView;
use serde::{Deserialize, Serialize};
use wvq_intelligence::plan_cheapest_evidence;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheapestEvidenceDocument {
    schema_v: u32,
    revision: String,
    truncated: bool,
    gaps: Vec<wvq_intelligence::EvidencePlan>,
}

#[cfg(test)]
pub(in crate::service) fn persist_cheapest_evidence_plan(
    store: &Store,
    run: &RunId,
    revision: &RevisionId,
    graph: &Value,
    records: &[ExecutorRecord],
    bindings: &[TestBinding],
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let plan = cheapest_evidence_document(graph, records, bindings)?;
    let document = CheapestEvidenceDocument {
        schema_v: 1,
        revision: revision.to_string(),
        truncated: plan.truncated,
        gaps: plan.gaps,
    };
    put_json_run_artifact(
        store,
        run,
        &format!("artifact-{}-cheapest-evidence-plan", run.as_str()),
        CHEAPEST_EVIDENCE_PLAN_KIND,
        &document,
        handles,
    )
}

pub(in crate::service) fn persist_cheapest_evidence_from(
    store: &Store,
    run: &RunId,
    revision: &RevisionId,
    sources: &SurfaceEvidenceSources<'_>,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let matrix = surface_evidence_from(sources)?;
    let plan = plan_cheapest_evidence(&matrix);
    let document = CheapestEvidenceDocument {
        schema_v: 1,
        revision: revision.to_string(),
        truncated: plan.truncated,
        gaps: plan.gaps,
    };
    put_json_run_artifact(
        store,
        run,
        &format!("artifact-{}-cheapest-evidence-plan", run.as_str()),
        CHEAPEST_EVIDENCE_PLAN_KIND,
        &document,
        handles,
    )
}

#[cfg(test)]
pub(in crate::service) fn cheapest_evidence_document(
    graph: &Value,
    records: &[ExecutorRecord],
    bindings: &[TestBinding],
) -> Result<wvq_intelligence::CheapestEvidencePlan, BusError> {
    let matrix = surface_evidence_document(graph, records, bindings)?;
    Ok(plan_cheapest_evidence(&matrix))
}

pub(in crate::service) fn load_cheapest_evidence_plan(
    store: &Store,
    run: &RunId,
) -> Result<CheapestEvidencePlanView, BusError> {
    match read_single_run_json(store, run, CHEAPEST_EVIDENCE_PLAN_KIND) {
        Ok(value) => Ok(view_from_json(&value)?),
        Err(BusError::Store(message)) if message.contains("has no ") => {
            Ok(CheapestEvidencePlanView::absent())
        }
        Err(err) => Err(err),
    }
}

fn view_from_json(value: &Value) -> Result<CheapestEvidencePlanView, BusError> {
    let schema_v = value.get("schema_v").and_then(Value::as_u64).ok_or_else(|| {
        BusError::Store("cheapest-evidence-plan omitted schema_v".into())
    })?;
    if schema_v != 1 {
        return Err(BusError::Store(format!(
            "unknown cheapest-evidence-plan schema version {schema_v}"
        )));
    }
    let document: CheapestEvidenceDocument = serde_json::from_value(value.clone()).map_err(|err| {
        BusError::Store(format!("malformed cheapest-evidence-plan: {err}"))
    })?;
    Ok(CheapestEvidencePlanView {
        present: true,
        truncated: document.truncated,
        gaps: document.gaps,
    })
}
