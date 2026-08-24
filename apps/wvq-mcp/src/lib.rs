//! Agent-only MCP surface. Seven default tools. No shell. No Studio.

#![forbid(unsafe_code)]

mod authoring;
mod protection;
mod recovery;

pub use authoring::authoring_server;
pub use protection::{SharedProtection, protection_server};
pub use recovery::{SharedDesk, recovery_server};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use std::{collections::BTreeMap, path::PathBuf};

use mcport::{ConcurrentMcpServer, ConcurrentToolServer, RuntimeConfig, ToolReply, Value, json};
use serde::Serialize;
use wvq_command_bus::{
    BusError, ContextCommand, EvidenceCommand, ExplainCommand, PlanCommand, QualityService,
    RunCommand, StatusCommand, VerifyCommand, estimate_tokens,
};

/// MCP profile selected by the stdio host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostProfile {
    /// Seven-tool coding-agent profile.
    Default,
    /// Six-tool human-reviewed brownfield recovery profile.
    Recovery,
    /// Three-tool base/head protection continuity profile.
    Protection,
    /// Six-tool Playwright `TestProgram` authoring and passive-recording profile.
    Authoring,
}

/// Strict host options. These affect startup only and never become shell argv.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostOptions {
    /// Repository root.
    pub repo: PathBuf,
    /// Tool profile.
    pub profile: HostProfile,
    /// Recovery change identity.
    pub change: String,
    /// Immutable base ref.
    pub base: String,
    /// Working tree or checked-out commit.
    pub head: String,
}

/// Parse the stdio host arguments.
///
/// # Errors
///
/// Unknown, repeated, or incomplete options are rejected.
pub fn parse_host_args(args: &[String]) -> Result<HostOptions, String> {
    let mut flags = BTreeMap::new();
    let mut index = 0;
    while index < args.len() {
        let name = args[index]
            .strip_prefix("--")
            .ok_or_else(|| format!("unexpected argument `{}`", args[index]))?;
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("option --{name} requires a value"))?;
        if flags.insert(name.to_owned(), value.clone()).is_some() {
            return Err(format!("option --{name} was supplied more than once"));
        }
        index += 2;
    }
    if let Some(unknown) = flags
        .keys()
        .find(|name| !["repo", "profile", "change", "base", "head"].contains(&name.as_str()))
    {
        return Err(format!("unknown option --{unknown}"));
    }
    let profile = match flags.get("profile").map_or("default", String::as_str) {
        "default" => HostProfile::Default,
        "recovery" => HostProfile::Recovery,
        "protection" => HostProfile::Protection,
        "authoring" => HostProfile::Authoring,
        other => return Err(format!("unknown MCP profile `{other}`")),
    };
    Ok(HostOptions {
        repo: flags
            .get("repo")
            .map_or_else(|| PathBuf::from("."), PathBuf::from),
        profile,
        change: flags
            .get("change")
            .cloned()
            .unwrap_or_else(|| "current".into()),
        base: flags.get("base").cloned().unwrap_or_else(|| "HEAD".into()),
        head: flags
            .get("head")
            .cloned()
            .unwrap_or_else(|| "WORKTREE".into()),
    })
}

/// Controlled concurrency and handler deadlines for the default profile.
#[must_use]
pub fn runtime_config() -> RuntimeConfig {
    RuntimeConfig {
        max_in_flight: 4,
        queue_depth: 32,
        output_queue_depth: 32,
        // `quality_run` uses bounded repository runners whose own hard limit is
        // 15 minutes. The transport must not cancel a valid run first.
        handler_deadline: Some(Duration::from_secs(16 * 60)),
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
            "Execute repository-discovered registered runners for an explicit base/head range with bounded argv, deadline, output, CAS evidence, and revision checks. Impacted widens to all until selection evidence is complete. No arbitrary shell.",
            schema_run(),
            move |ctx, input: RunCommand| {
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
                let reply = run.run_controlled(&input, cancel);
                completed.store(true, Ordering::Release);
                let _ = watcher.join();
                tool_result(reply)
            },
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
            "base": {
                "type": "string",
                "description": "Immutable Git base ref. Defaults to HEAD for working-tree analysis."
            },
            "head": {
                "type": "string",
                "description": "WORKTREE, or a commit ref that must equal the checked-out clean HEAD."
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
