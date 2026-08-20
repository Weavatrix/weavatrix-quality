//! Agent-only MCP surface. Seven default tools. No shell. No Studio.

#![forbid(unsafe_code)]

mod protection;
mod recovery;

pub use protection::{SharedProtection, protection_server};
pub use recovery::{SharedDesk, recovery_server};

use std::sync::Arc;
use std::time::Duration;

use mcport::{ConcurrentMcpServer, ConcurrentToolServer, RuntimeConfig, ToolReply, Value, json};
use serde::Serialize;
use wvq_command_bus::{
    BusError, ContextCommand, EvidenceCommand, ExplainCommand, PlanCommand, QualityService,
    RunCommand, StatusCommand, VerifyCommand, estimate_tokens,
};

/// Controlled concurrency and handler deadlines for the default profile.
#[must_use]
pub fn runtime_config() -> RuntimeConfig {
    RuntimeConfig {
        max_in_flight: 4,
        queue_depth: 32,
        output_queue_depth: 32,
        handler_deadline: Some(Duration::from_secs(30)),
        ..RuntimeConfig::default()
    }
}

/// Seven-tool coding-agent server. Schemas are strict.
#[must_use]
pub fn quality_server(service: &Arc<dyn QualityService>) -> ConcurrentMcpServer {
    let context = Arc::clone(service);
    let plan = Arc::clone(service);
    let run = Arc::clone(service);
    let status = Arc::clone(service);
    let verify = Arc::clone(service);
    let explain = Arc::clone(service);
    let evidence = Arc::clone(service);
    ConcurrentMcpServer::new("weavatrix-quality", env!("CARGO_PKG_VERSION"))
        .instructions(
            "Weavatrix Quality. Seven bounded tools. No shell. Large evidence is a handle.",
        )
        .strict_schemas()
        .typed_tool(
            "quality_context",
            "Bounded neighbouring requirements, obligations, and heuristics for one change.",
            schema_context(),
            move |_ctx, input: ContextCommand| tool_result(context.context(&input)),
        )
        .typed_tool(
            "quality_plan",
            "Requirements, obligations, risk evidence, proofs, and gaps. Does not execute.",
            schema_change_only(),
            move |_ctx, input: PlanCommand| tool_result(plan.plan(&input)),
        )
        .typed_tool(
            "quality_run",
            "Execute the bounded selected plan through registered executors. No arbitrary shell.",
            schema_run(),
            move |_ctx, input: RunCommand| tool_result(run.run(&input)),
        )
        .typed_tool(
            "quality_status",
            "Compact run progress plus artifact handles.",
            schema_status(),
            move |_ctx, input: StatusCommand| tool_result(status.status(&input)),
        )
        .typed_tool(
            "quality_verify",
            "Assemble the revision-bound multi-axis quality verdict.",
            schema_change_only(),
            move |_ctx, input: VerifyCommand| tool_result(verify.verify(&input)),
        )
        .typed_tool(
            "quality_explain",
            "Explain one finding, failure, selection, or proof with exact provenance.",
            schema_explain(),
            move |_ctx, input: ExplainCommand| tool_result(explain.explain(&input)),
        )
        .typed_tool(
            "quality_evidence",
            "Return bounded metadata or small text. Large binary data stays a handle.",
            schema_evidence(),
            move |_ctx, input: EvidenceCommand| tool_result(evidence.evidence(&input)),
        )
}

/// Approximate token cost of the advertised tool catalog.
#[must_use]
pub fn catalog_token_footprint(server: &ConcurrentMcpServer) -> u64 {
    estimate_tokens(&server.catalog().to_string())
}

fn tool_result<T: Serialize>(result: Result<T, BusError>) -> ToolReply {
    match result {
        Ok(value) => ToolReply::structured(value),
        Err(err) => ToolReply::error(err.to_string()),
    }
}

fn schema_context() -> Value {
    json!({
        "type": "object",
        "properties": {
            "change": {
                "type": "string",
                "description": "Change id, or `current` when exactly one OpenSpec change exists."
            },
            "purpose": {
                "type": "string",
                "description": "Why context is requested: spec, implementation, or review."
            },
            "token_budget": {
                "type": "integer",
                "minimum": 1,
                "description": "Maximum approximate tokens in the QualityContextPacket."
            }
        },
        "additionalProperties": false
    })
}

fn schema_change_only() -> Value {
    json!({
        "type": "object",
        "properties": {
            "change": {
                "type": "string",
                "description": "Change id, or `current` when unambiguous."
            }
        },
        "additionalProperties": false
    })
}

fn schema_run() -> Value {
    json!({
        "type": "object",
        "properties": {
            "change": {
                "type": "string",
                "description": "Change id, or `current` when unambiguous."
            },
            "scope": {
                "type": "string",
                "description": "Execution scope: impacted or all."
            },
            "evidence_policy": {
                "type": "string",
                "description": "Evidence collection policy: standard, minimal, or none."
            }
        },
        "additionalProperties": false
    })
}

fn schema_status() -> Value {
    json!({
        "type": "object",
        "properties": {
            "run_id": {
                "type": "string",
                "description": "Run identity. Omit to read the latest run."
            }
        },
        "additionalProperties": false
    })
}

fn schema_explain() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "string",
                "description": "Finding, proof, selection, or obligation identity."
            }
        },
        "required": ["id"],
        "additionalProperties": false
    })
}

fn schema_evidence() -> Value {
    json!({
        "type": "object",
        "properties": {
            "handle": {
                "type": "string",
                "description": "CAS handle or artifact id. Bytes are not dumped into context."
            }
        },
        "required": ["handle"],
        "additionalProperties": false
    })
}
