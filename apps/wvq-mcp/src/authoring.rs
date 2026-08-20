//! Opt-in Playwright `TestProgram` authoring profile.
//!
//! Agents receive high-level draft/validate/preview operations. They never get
//! a generic browser control, JavaScript evaluation, shell, or oracle mutation.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use mcport::{ConcurrentMcpServer, ToolReply, Value, json};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use wvq_command_bus::{
    AuthorDraftCommand, AuthorPreviewCommand, AuthorValidateCommand, BusError, QualityService,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftInput {
    #[serde(default = "default_token_budget")]
    token_budget: u64,
    #[serde(default)]
    use_model: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateInput {
    program: JsonValue,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewInput {
    program: JsonValue,
    #[serde(default = "default_true")]
    screenshot: bool,
    #[serde(default)]
    trace: bool,
}

fn default_token_budget() -> u64 {
    8_000
}

fn default_true() -> bool {
    true
}

/// Three high-level tools, fixed to one startup-selected change and Git range.
#[must_use]
pub fn authoring_server(
    service: &Arc<dyn QualityService>,
    change: &str,
    base: &str,
    head: &str,
) -> ConcurrentMcpServer {
    let draft_service = Arc::clone(service);
    let validate_service = Arc::clone(service);
    let preview_service = Arc::clone(service);
    let draft_change = change.to_owned();
    let validate_change = change.to_owned();
    let preview_change = change.to_owned();
    let draft_base = base.to_owned();
    let preview_base = base.to_owned();
    let draft_head = head.to_owned();
    let preview_head = head.to_owned();
    ConcurrentMcpServer::new("weavatrix-quality-authoring", env!("CARGO_PKG_VERSION"))
        .instructions(
            "Author Playwright-backed TestPrograms against changed code and sealed obligations. Candidates never rewrite or seal intent. Preview is the only browser side effect.",
        )
        .strict_schemas()
        .typed_tool(
            "quality_test_draft",
            "Return complete sealed obligations plus bounded changed-code and Weavatrix graph context. Optionally request one budgeted local-model candidate.",
            schema_draft(),
            move |_ctx, input: DraftInput| {
                tool_result(draft_service.author_draft(&AuthorDraftCommand {
                    change: draft_change.clone(),
                    base: draft_base.clone(),
                    head: draft_head.clone(),
                    token_budget: input.token_budget,
                    use_model: input.use_model,
                }))
            },
        )
        .typed_tool(
            "quality_test_validate",
            "Strictly validate one canonical TestProgram against the existing OracleSeal. Does not write files or register the program.",
            schema_validate(),
            move |_ctx, input: ValidateInput| {
                tool_result(validate_service.author_validate(&AuthorValidateCommand {
                    change: validate_change.clone(),
                    program: input.program,
                }))
            },
        )
        .typed_tool(
            "quality_test_preview",
            "Run one validated candidate through configured Playwright and return observation, screenshot, and optional trace handles. Does not save the candidate.",
            schema_preview(),
            move |ctx, input: PreviewInput| {
                let cancel = Arc::new(AtomicBool::new(false));
                let completed = Arc::new(AtomicBool::new(false));
                let watcher_cancel = Arc::clone(&cancel);
                let watcher_completed = Arc::clone(&completed);
                let watcher_context = ctx.clone();
                let watcher = thread::spawn(move || {
                    while !watcher_completed.load(Ordering::Acquire) {
                        if watcher_context.is_cancelled() {
                            watcher_cancel.store(true, Ordering::Release);
                            break;
                        }
                        thread::sleep(Duration::from_millis(25));
                    }
                });
                let reply = preview_service.author_preview_controlled(
                    &AuthorPreviewCommand {
                        change: preview_change.clone(),
                        base: preview_base.clone(),
                        head: preview_head.clone(),
                        program: input.program,
                        screenshot: input.screenshot,
                        trace: input.trace,
                    },
                    cancel,
                );
                completed.store(true, Ordering::Release);
                let _ = watcher.join();
                tool_result(reply)
            },
        )
}

fn tool_result<T: Serialize>(result: Result<T, BusError>) -> ToolReply {
    match result {
        Ok(value) => ToolReply::structured(value),
        Err(err) => ToolReply::error(err.to_string()),
    }
}

fn schema_draft() -> Value {
    json!({
        "type": "object",
        "properties": {
            "token_budget": {
                "type": "integer",
                "minimum": 256,
                "maximum": 64000,
                "description": "Approximate packet token budget. Complete sealed authority is never truncated."
            },
            "use_model": {
                "type": "boolean",
                "description": "Explicitly spend the configured planning-model budget for a candidate. Defaults false."
            }
        },
        "additionalProperties": false
    })
}

fn schema_validate() -> Value {
    json!({
        "type": "object",
        "properties": {
            "program": {
                "type": "object",
                "description": "Canonical schema_v=1 TestProgram JSON object; the command bus performs strict IR validation.",
                "additionalProperties": true
            }
        },
        "required": ["program"],
        "additionalProperties": false
    })
}

fn schema_preview() -> Value {
    json!({
        "type": "object",
        "properties": {
            "program": {
                "type": "object",
                "description": "Canonical schema_v=1 TestProgram JSON object; the command bus performs strict IR validation.",
                "additionalProperties": true
            },
            "screenshot": {
                "type": "boolean",
                "description": "Capture a screenshot after each attempted step. Defaults true."
            },
            "trace": {
                "type": "boolean",
                "description": "Capture a Playwright trace. Defaults false."
            }
        },
        "required": ["program"],
        "additionalProperties": false
    })
}
