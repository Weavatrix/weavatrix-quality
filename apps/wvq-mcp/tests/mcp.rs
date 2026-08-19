//! Task 16: seven default tools, strict schemas, handle-based evidence, catalog budget.

use std::sync::Arc;
use std::time::Duration;

use mcport::{ConcurrentToolServer, RuntimeConfig, json};
use wvq_command_bus::{FakeService, INLINE_LIMIT, QualityService, estimate_tokens};
use wvq_mcp::{catalog_token_footprint, quality_server, runtime_config};

const DEFAULT_TOOLS: [&str; 7] = [
    "quality_context",
    "quality_plan",
    "quality_run",
    "quality_status",
    "quality_verify",
    "quality_explain",
    "quality_evidence",
];

fn server() -> mcport::ConcurrentMcpServer {
    let service: Arc<dyn QualityService> = Arc::new(FakeService::default());
    quality_server(&service)
}

#[test]
fn default_profile_exposes_exactly_seven_tools() {
    let catalog = server().catalog();
    let tools = catalog.as_array().expect("tool catalog is an array");
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| {
            tool.get("name")
                .and_then(mcport::Value::as_str)
                .unwrap_or("")
        })
        .collect();
    assert_eq!(names, DEFAULT_TOOLS);
    for banned in [
        "quality_select",
        "quality_explore",
        "quality_mutate",
        "quality_replay",
        "browser_click",
        "shell",
    ] {
        assert!(
            !names.contains(&banned),
            "default profile must not expose {banned}"
        );
    }
}

#[test]
fn strict_schemas_have_no_defects() {
    let built = server();
    assert!(
        built.schema_defects().is_empty(),
        "strict catalog defects: {:?}",
        built.schema_defects()
    );
}

#[test]
fn tool_schema_token_footprint_stays_small() {
    let built = server();
    let tokens = catalog_token_footprint(&built);
    let bytes = built.catalog().to_string().len();
    assert!(
        tokens < 2_000,
        "catalog token footprint {tokens} (bytes {bytes}) is too large for a coding-agent profile"
    );
    assert!(tokens > 50, "catalog should describe the seven tools");
}

#[test]
fn runtime_uses_controlled_concurrency_and_deadlines() {
    let config = runtime_config();
    assert_eq!(config.max_in_flight, 4);
    assert_eq!(config.queue_depth, 32);
    assert_eq!(config.handler_deadline, Some(Duration::from_secs(30)));
    let _ = RuntimeConfig::default();
}

#[test]
fn quality_context_is_bounded() {
    let fake = FakeService::default();
    fake.set_context_items(
        (0..40)
            .map(|index| format!("requirement clause {index} with neighbouring intent text"))
            .collect(),
    );
    let reply = fake
        .context(&wvq_command_bus::ContextCommand {
            change: "sankey-others".into(),
            purpose: "spec".into(),
            token_budget: 12,
        })
        .unwrap();
    assert!(reply.truncated);
    assert!(reply.tokens_used <= 12);
}

#[test]
fn quality_evidence_keeps_large_blobs_as_handles() {
    let fake = FakeService::default();
    fake.put_evidence("shot-1", vec![0_u8; INLINE_LIMIT + 8]);
    let reply = fake
        .evidence(&wvq_command_bus::EvidenceCommand {
            handle: "shot-1".into(),
        })
        .unwrap();
    assert!(reply.inline_text.is_none());
    let encoded = serde_json::to_string(&reply).unwrap();
    assert!(
        encoded.len() < 1_000,
        "large evidence must not dump blob bytes into the MCP reply"
    );
    assert!(estimate_tokens(&encoded) < 200);
}

#[test]
fn catalog_descriptors_are_honest_objects() {
    let catalog = server().catalog();
    for tool in catalog.as_array().unwrap() {
        let schema = tool.get("inputSchema").expect("inputSchema");
        assert_eq!(
            schema.get("type").and_then(mcport::Value::as_str),
            Some("object")
        );
        assert!(schema.get("properties").is_some());
        let _ = json!({"ok": true});
    }
}
