//! Advanced protection-continuity MCP profile. Spec §87.
//!
//! Three tools that answer "what protected this before, and what protects it
//! now?". Like the recovery profile, they stay off the default seven-tool
//! coding-agent surface.

use std::sync::{Arc, Mutex, MutexGuard};

use mcport::{ConcurrentMcpServer, ToolReply, Value, json};
use serde::Deserialize;
use wvq_proof::ProtectionView;

/// Shared protection view. The host computes it from both revisions.
pub type SharedProtection = Arc<Mutex<ProtectionView>>;

fn lock(view: &SharedProtection) -> MutexGuard<'_, ProtectionView> {
    view.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestInput {
    test: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct FlowInput {
    flow: String,
}

/// Three-tool protection profile over one shared view.
#[must_use]
pub fn protection_server(view: &SharedProtection) -> ConcurrentMcpServer {
    let protection = Arc::clone(view);
    let lineage = Arc::clone(view);
    let flow = Arc::clone(view);
    ConcurrentMcpServer::new("weavatrix-quality-protection", env!("CARGO_PKG_VERSION"))
        .instructions(
            "Protection continuity. A global coverage gain never offsets a local protection loss.",
        )
        .strict_schemas()
        .typed_tool(
            "quality_protection",
            "Preserved, improved, degraded, lost and unprotected flows, plus blocking findings.",
            schema_empty(),
            move |_ctx, _input: Value| ToolReply::structured(lock(&protection).report()),
        )
        .typed_tool(
            "quality_test_lineage",
            "What happened to one test, and whether its protection changed with it.",
            schema_test(),
            move |_ctx, input: TestInput| match lock(&lineage).lineage_of(&input.test) {
                Some(record) => ToolReply::structured(record),
                None => ToolReply::error(format!("no lineage recorded for `{}`", input.test)),
            },
        )
        .typed_tool(
            "quality_flow",
            "One flow before and after: path, tests, coverage and proof on each revision.",
            schema_flow(),
            move |_ctx, input: FlowInput| match lock(&flow).flow(&input.flow) {
                Some(record) => ToolReply::structured(record),
                None => {
                    ToolReply::error(format!("no flow `{}` in the impacted surface", input.flow))
                }
            },
        )
}

fn schema_empty() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn schema_test() -> Value {
    json!({
        "type": "object",
        "properties": {
            "test": { "type": "string", "description": "Test identity to explain." }
        },
        "required": ["test"],
        "additionalProperties": false
    })
}

fn schema_flow() -> Value {
    json!({
        "type": "object",
        "properties": {
            "flow": { "type": "string", "description": "Impacted flow identity." }
        },
        "required": ["flow"],
        "additionalProperties": false
    })
}
