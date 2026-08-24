//! Opt-in Playwright `TestProgram` authoring profile.
//!
//! Agents receive high-level draft/validate/preview operations. They never get
//! a generic browser control, JavaScript evaluation, shell, or oracle mutation.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use mcport::{ConcurrentMcpServer, ToolReply, Value, json};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use wvq_command_bus::{
    AuthorDraftCommand, AuthorHealCommand, AuthorHealEdit, AuthorPreviewCommand,
    AuthorPromoteCommand, AuthorValidateCommand, BusError, QualityService, RecordCommand,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PromoteInput {
    preview_id: String,
    program: JsonValue,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordInput {
    #[serde(default = "default_route")]
    route: String,
    #[serde(default)]
    fixture_values: BTreeMap<String, String>,
    #[serde(default = "default_idle_timeout_ms")]
    idle_timeout_ms: u64,
    #[serde(default = "default_max_events")]
    max_events: u32,
    #[serde(default)]
    headless: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct HealInput {
    program_id: String,
    expected_program_revision: u32,
    edits: Vec<AuthorHealEdit>,
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

fn default_route() -> String {
    "/".into()
}

fn default_idle_timeout_ms() -> u64 {
    3_000
}

fn default_max_events() -> u32 {
    200
}

/// Six high-level tools, fixed to one startup-selected change and Git range.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn authoring_server(
    service: &Arc<dyn QualityService>,
    change: &str,
    base: &str,
    head: &str,
) -> ConcurrentMcpServer {
    let draft_service = Arc::clone(service);
    let validate_service = Arc::clone(service);
    let preview_service = Arc::clone(service);
    let promote_service = Arc::clone(service);
    let record_service = Arc::clone(service);
    let heal_service = Arc::clone(service);
    let draft_change = change.to_owned();
    let validate_change = change.to_owned();
    let preview_change = change.to_owned();
    let promote_change = change.to_owned();
    let record_change = change.to_owned();
    let heal_change = change.to_owned();
    let draft_base = base.to_owned();
    let preview_base = base.to_owned();
    let record_base = base.to_owned();
    let heal_base = base.to_owned();
    let draft_head = head.to_owned();
    let preview_head = head.to_owned();
    let record_head = head.to_owned();
    let heal_head = head.to_owned();
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
        .typed_tool(
            "quality_test_promote",
            "Persist the exact canonical TestProgram from one passing same-revision preview. Never creates or changes an OracleSeal.",
            schema_promote(),
            move |_ctx, input: PromoteInput| {
                tool_result(promote_service.author_promote(&AuthorPromoteCommand {
                    change: promote_change.clone(),
                    preview_id: input.preview_id,
                    program: input.program,
                }))
            },
        )
        .typed_tool(
            "quality_test_record",
            "Open a bounded Playwright session, passively capture semantic natural use, discard redundant traces, and return a sealed reviewable replay candidate when useful. Unknown form values are not captured.",
            schema_record(),
            move |ctx, input: RecordInput| {
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
                let reply = record_service.record_controlled(
                    &RecordCommand {
                        change: record_change.clone(),
                        base: record_base.clone(),
                        head: record_head.clone(),
                        route: input.route,
                        fixture_values: input.fixture_values,
                        idle_timeout_ms: input.idle_timeout_ms,
                        max_events: input.max_events,
                        headless: input.headless,
                    },
                    cancel,
                );
                completed.store(true, Ordering::Release);
                let _ = watcher.join();
                tool_result(reply)
            },
        )
        .typed_tool(
            "quality_test_heal",
            "Apply locator-alias or deterministic-wait edits to one persisted TestProgram, replay the original sealed assertions, and append a version only on pass.",
            schema_heal(),
            move |ctx, input: HealInput| {
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
                let reply = heal_service.author_heal_controlled(
                    &AuthorHealCommand {
                        change: heal_change.clone(),
                        base: heal_base.clone(),
                        head: heal_head.clone(),
                        program_id: input.program_id,
                        expected_program_revision: input.expected_program_revision,
                        edits: input.edits,
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

fn schema_promote() -> Value {
    json!({
        "type": "object",
        "properties": {
            "preview_id": {
                "type": "string",
                "minLength": 1,
                "description": "Passing preview identity returned by quality_test_preview."
            },
            "program": {
                "type": "object",
                "description": "The exact canonical TestProgram exercised by the preview.",
                "additionalProperties": true
            }
        },
        "required": ["preview_id", "program"],
        "additionalProperties": false
    })
}

fn schema_record() -> Value {
    json!({
        "type": "object",
        "properties": {
            "route": {
                "type": "string",
                "pattern": "^/[^/].*|^/$",
                "description": "Same-origin root-relative route. Defaults to /."
            },
            "fixture_values": {
                "type": "object",
                "maxProperties": 256,
                "additionalProperties": {"type": "string", "maxLength": 8192},
                "description": "Explicit safe values keyed by replay fixture name. Unknown form values are redacted."
            },
            "idle_timeout_ms": {
                "type": "integer",
                "minimum": 50,
                "maximum": 60000
            },
            "max_events": {
                "type": "integer",
                "minimum": 1,
                "maximum": 1000
            },
            "headless": {
                "type": "boolean",
                "description": "Defaults false so a human can naturally use the page."
            }
        },
        "additionalProperties": false
    })
}

fn schema_heal() -> Value {
    json!({
        "type": "object",
        "properties": {
            "program_id": {
                "type": "string",
                "minLength": 1,
                "description": "Persisted TestProgram identity."
            },
            "expected_program_revision": {
                "type": "integer",
                "minimum": 1,
                "description": "Latest version observed by the caller; stale writes fail."
            },
            "edits": {
                "type": "array",
                "minItems": 1,
                "maxItems": 64,
                "items": {
                    "oneOf": [
                        {
                            "type": "object",
                            "properties": {
                                "edit": {"type": "string", "const": "retarget"},
                                "step": {"type": "integer", "minimum": 0},
                                "target": {"type": "object", "additionalProperties": true}
                            },
                            "required": ["edit", "step", "target"],
                            "additionalProperties": false
                        },
                        {
                            "type": "object",
                            "properties": {
                                "edit": {"type": "string", "const": "insert_wait"},
                                "after": {"type": "integer", "minimum": 0},
                                "condition": {"type": "object", "additionalProperties": true}
                            },
                            "required": ["edit", "after", "condition"],
                            "additionalProperties": false
                        }
                    ]
                }
            },
            "screenshot": {"type": "boolean"},
            "trace": {"type": "boolean"}
        },
        "required": ["program_id", "expected_program_revision", "edits"],
        "additionalProperties": false
    })
}
