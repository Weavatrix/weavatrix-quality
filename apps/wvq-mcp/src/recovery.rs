//! Advanced spec-recovery MCP profile. Spec §89 Task 28.
//!
//! These six tools are **not** part of the default coding-agent surface: that
//! stays at seven tools so its schema footprint stays small. A host opts into
//! this profile when a human is actually going to review recovered intent.

use std::sync::{Arc, Mutex, MutexGuard};

use mcport::{ConcurrentMcpServer, ToolReply, Value, json};
use serde::{Deserialize, Serialize};
use wvq_domain::{
    ContentHash, HumanDecision, HumanDecisionId, HumanRole, NewDecision, VerificationDecision,
};
use wvq_spec_recovery::RecoveryDesk;

/// Shared recovery desk. The host populates it from repository evidence.
pub type SharedDesk = Arc<Mutex<RecoveryDesk>>;

fn lock(desk: &SharedDesk) -> MutexGuard<'_, RecoveryDesk> {
    desk.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A reviewer's decision, as it arrives over MCP.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifyInput {
    id: String,
    reviewer: String,
    role: HumanRole,
    /// Candidate being decided. Exactly one; there is no bulk form.
    candidate: String,
    artifact_digest: String,
    decision: VerificationDecision,
    #[serde(default)]
    comment: Option<String>,
    decided_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SealInput {
    candidate: String,
}

#[derive(Debug, Clone, Serialize)]
struct StateReply {
    candidate: String,
    state: &'static str,
}

/// Six-tool recovery profile over one shared desk.
#[must_use]
pub fn recovery_server(desk: &SharedDesk) -> ConcurrentMcpServer {
    let recover = Arc::clone(desk);
    let review = Arc::clone(desk);
    let questions = Arc::clone(desk);
    let patch = Arc::clone(desk);
    let verify = Arc::clone(desk);
    let seal = Arc::clone(desk);
    ConcurrentMcpServer::new("weavatrix-quality-spec-recovery", env!("CARGO_PKG_VERSION"))
        .instructions(
            "Recovered intent is a proposal. Nothing here seals without human verification.",
        )
        .strict_schemas()
        .typed_tool(
            "quality_spec_recover",
            "Return the bounded RecoveryPacket: narrative, clusters, surface delta, heuristics.",
            schema_empty(),
            move |_ctx, _input: Value| match lock(&recover).packet() {
                Some(packet) => ToolReply::structured(packet),
                None => ToolReply::error("no recovery has been prepared for this change"),
            },
        )
        .typed_tool(
            "quality_spec_review",
            "Candidate requirements with state, evidence and deterministic findings.",
            schema_empty(),
            move |_ctx, _input: Value| ToolReply::structured(lock(&review).review()),
        )
        .typed_tool(
            "quality_spec_questions",
            "Adaptive questions for QA, and anything escalated to product.",
            schema_empty(),
            move |_ctx, _input: Value| ToolReply::structured(lock(&questions).questions()),
        )
        .typed_tool(
            "quality_spec_preview_patch",
            "Render the proposed OpenSpec patch. Always labelled PROPOSED.",
            schema_empty(),
            move |_ctx, _input: Value| {
                ToolReply::structured(json!({ "patch": lock(&patch).preview_patch() }))
            },
        )
        .typed_tool(
            "quality_spec_verify",
            "Record one human decision against exactly one candidate.",
            schema_verify(),
            move |_ctx, input: VerifyInput| {
                let candidate = input.candidate.clone();
                match build_decision(input) {
                    Ok(decision) => match lock(&verify).decide(&decision) {
                        Ok(state) => ToolReply::structured(StateReply {
                            candidate,
                            state: state.as_str(),
                        }),
                        Err(err) => ToolReply::error(err.to_string()),
                    },
                    Err(err) => ToolReply::error(err),
                }
            },
        )
        .typed_tool(
            "quality_spec_seal",
            "Seal one verified candidate. Refuses without the mandatory approvals.",
            schema_seal(),
            move |_ctx, input: SealInput| match lock(&seal).seal(&input.candidate) {
                Ok(approval) => ToolReply::structured(approval),
                Err(err) => ToolReply::error(err.to_string()),
            },
        )
}

fn build_decision(input: VerifyInput) -> Result<HumanDecision, String> {
    let id = HumanDecisionId::new(&input.id).map_err(|err| err.to_string())?;
    let digest = ContentHash::new(&input.artifact_digest).map_err(|err| err.to_string())?;
    HumanDecision::new(NewDecision {
        id,
        reviewer: input.reviewer,
        role: input.role,
        subject: input.candidate,
        artifact_digest: digest,
        decision: input.decision,
        comment: input.comment,
        decided_at: input.decided_at,
    })
    .map_err(|err| err.to_string())
}

fn schema_empty() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn schema_verify() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string", "description": "Decision identity." },
            "reviewer": { "type": "string", "description": "Reviewer identity." },
            "role": { "type": "string", "enum": ["qa", "product", "developer"] },
            "candidate": {
                "type": "string",
                "description": "Exactly one candidate. There is no bulk accept-all form."
            },
            "artifact_digest": {
                "type": "string",
                "description": "Digest of the candidate the reviewer saw. A stale digest is refused."
            },
            "decision": {
                "type": "string",
                "enum": [
                    "accept_as_intended", "edit", "reject", "observed_only", "add_scenario",
                    "mark_duplicate", "mark_non_behavioral", "request_product_decision",
                    "request_developer_clarification"
                ]
            },
            "comment": { "type": "string" },
            "decided_at": { "type": "string", "description": "Host-supplied timestamp." }
        },
        "required": ["id", "reviewer", "role", "candidate", "artifact_digest", "decision", "decided_at"],
        "additionalProperties": false
    })
}

fn schema_seal() -> Value {
    json!({
        "type": "object",
        "properties": {
            "candidate": { "type": "string", "description": "Candidate to seal." }
        },
        "required": ["candidate"],
        "additionalProperties": false
    })
}
