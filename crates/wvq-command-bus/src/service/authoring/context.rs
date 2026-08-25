//! Bounded authoring context assembled from OpenSpec, Git, and Weavatrix.

use serde_json::{Value, json};
use wvq_spec::{OpenSpecChange, RequirementOp, TestObligation};

use super::super::{BusError, graph_node_id, values_at};
use super::tokens::{evidence_kind_token, obligation_kind_token, risk_token};
use crate::replies::{AuthorDraftReply, AuthoringObligation, ContextReply, bound_items, estimate_tokens};

pub(in crate::service) fn authoring_obligations(
    obligations: &[TestObligation],
) -> Result<Vec<AuthoringObligation>, BusError> {
    obligations
        .iter()
        .map(|item| {
            Ok(AuthoringObligation {
                id: item.id.to_string(),
                requirement: item.requirement.to_string(),
                scenario: item.scenario.to_string(),
                kind: obligation_kind_token(item.kind).into(),
                risk: risk_token(item.risk).into(),
                condition: item
                    .condition
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
                expected: item
                    .expected
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
                required_evidence: item
                    .required_evidence
                    .iter()
                    .map(|kind| evidence_kind_token(*kind).to_owned())
                    .collect(),
            })
        })
        .collect()
}

pub(in crate::service) fn authoring_authority_tokens(
    changed_files: &[String],
    obligations: &[AuthoringObligation],
) -> Result<u64, BusError> {
    let authority = serde_json::to_string(&json!({
        "changed_files": changed_files,
        "obligations": obligations,
    }))
    .map_err(|err| BusError::Runtime(err.to_string()))?;
    Ok(estimate_tokens(&authority).max(1))
}

pub(in crate::service) fn authoring_context(
    spec: &OpenSpecChange,
    changed_files: &[String],
    diff: &Value,
    impact: &Value,
) -> Vec<String> {
    let mut out = detailed_requirement_texts(spec);
    out.extend(
        changed_files
            .iter()
            .map(|path| format!("changed file: {path}")),
    );
    for (label, pointer) in [
        ("graph added", "/nodes/added"),
        ("graph removed", "/nodes/removed"),
    ] {
        out.extend(
            values_at(diff, pointer)
                .iter()
                .filter_map(graph_node_id)
                .map(|id| format!("{label}: {id}")),
        );
    }
    for item in values_at(diff, "/nodes/changed") {
        if let Some(id) = item.get("before").and_then(graph_node_id) {
            out.push(format!("graph changed base: {id}"));
        }
        if let Some(id) = item.get("after").and_then(graph_node_id) {
            out.push(format!("graph changed head: {id}"));
        }
    }
    out.extend(
        values_at(impact, "/impacted_nodes")
            .iter()
            .filter_map(graph_node_id)
            .map(|id| format!("graph impacted: {id}")),
    );
    out.sort();
    out.dedup();
    out
}

fn detailed_requirement_texts(spec: &OpenSpecChange) -> Vec<String> {
    let mut out = Vec::new();
    for capability in &spec.capabilities {
        for operation in &capability.operations {
            let delta = match operation {
                RequirementOp::Added(delta)
                | RequirementOp::Modified(delta)
                | RequirementOp::Removed(delta) => delta,
                RequirementOp::Renamed { from, to, location } => {
                    out.push(format!(
                        "intent rename at {}:{}: {from} -> {to}",
                        location.file.display(),
                        location.line
                    ));
                    continue;
                }
            };
            out.push(format!(
                "intent requirement {}: {} — {}",
                delta.id, delta.name, delta.text
            ));
            for scenario in &delta.scenarios {
                let clauses = scenario
                    .clauses
                    .iter()
                    .map(|clause| format!("{:?} {}", clause.kind, clause.text))
                    .collect::<Vec<_>>()
                    .join("; ");
                out.push(format!(
                    "intent scenario {} ({}) for {}: {clauses}",
                    scenario.id, scenario.name, delta.id
                ));
            }
        }
    }
    out
}

pub(in crate::service) fn authoring_model_prompt(reply: &AuthorDraftReply) -> Result<String, BusError> {
    let input = serde_json::to_value(reply).map_err(|err| BusError::Runtime(err.to_string()))?;
    serde_json::to_string(&json!({
        "task": "Return exactly one JSON object containing a canonical schema_v=1 TestProgram. Do not use markdown.",
        "rules": [
            "source must be generated",
            "only assert obligation ids whose expected field is non-null",
            "every declared obligation must have an assert step",
            "prefer semantic targets: test_id, role plus accessible_name, or label",
            "routes and api operation paths must be same-origin root-relative",
            "never invent an oracle, expected predicate, shell command, XPath, JavaScript, or filesystem write",
            "use only navigate, activate, fill, select, press, wait, set_feature_flag, inject_fault, api_call, assert"
        ],
        "test_program_shape": {
            "schema_v": 1,
            "id": "generated-program-id",
            "source": "generated",
            "obligations": ["sealed-obligation-id"],
            "preconditions": [],
            "steps": [{"action": "navigate", "route": "/"}, {"action": "assert", "obligation": "sealed-obligation-id"}],
            "data": {},
            "faults": {},
            "api_operations": {},
            "evidence_policy": {"screenshot": "on_failure", "trace": "on_failure", "network": "always", "console": "always", "storage": "on_failure"},
            "deterministic_seed": 1
        },
        "authoring_packet": input
    }))
    .map_err(|err| BusError::Runtime(err.to_string()))
}

pub(in crate::service) fn pack_context(change: &str, purpose: &str, budget: u64, items: Vec<String>) -> ContextReply {
    let (kept, used, truncated) = bound_items(items, budget.max(1));
    let mut requirements = Vec::new();
    let mut obligations = Vec::new();
    let mut heuristics = Vec::new();
    let mut coverage = Vec::new();
    for item in kept {
        if item.starts_with("obligation") {
            obligations.push(item);
        } else if item.starts_with("heuristic") {
            heuristics.push(item);
        } else if item.starts_with("coverage") {
            coverage.push(item);
        } else {
            requirements.push(item);
        }
    }
    ContextReply {
        change: change.to_owned(),
        purpose: purpose.to_owned(),
        requirements,
        obligations,
        heuristics,
        coverage,
        truncated,
        tokens_used: used,
        token_budget: budget.max(1),
    }
}

pub(in crate::service) fn requirement_texts(spec: &OpenSpecChange) -> Vec<String> {
    let mut out = Vec::new();
    for capability in &spec.capabilities {
        for operation in &capability.operations {
            let delta = match operation {
                RequirementOp::Added(delta)
                | RequirementOp::Modified(delta)
                | RequirementOp::Removed(delta) => delta,
                RequirementOp::Renamed { from, to, location } => {
                    out.push(format!(
                        "requirement rename {from} → {to} ({}:{})",
                        location.file.display(),
                        location.line
                    ));
                    continue;
                }
            };
            out.push(format!(
                "requirement {} at {}:{}: {}",
                delta.id,
                delta.location.file.display(),
                delta.location.line,
                delta.name
            ));
        }
    }
    out
}
