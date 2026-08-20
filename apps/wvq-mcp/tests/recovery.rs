//! Task 28: the recovery profile is separate from the default seven tools.

use std::sync::{Arc, Mutex};

use mcport::{ConcurrentToolServer, Value};
use wvq_command_bus::{FakeService, QualityService};
use wvq_mcp::{HostProfile, SharedDesk, parse_host_args, quality_server, recovery_server};
use wvq_spec_recovery::RecoveryDesk;

const RECOVERY_TOOLS: [&str; 6] = [
    "quality_spec_recover",
    "quality_spec_review",
    "quality_spec_questions",
    "quality_spec_preview_patch",
    "quality_spec_verify",
    "quality_spec_seal",
];

fn desk() -> SharedDesk {
    Arc::new(Mutex::new(RecoveryDesk::new("sankey-others")))
}

fn tool_names(catalog: &Value) -> Vec<String> {
    catalog
        .as_array()
        .expect("catalog is an array")
        .iter()
        .map(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .expect("tool has a name")
                .to_owned()
        })
        .collect()
}

#[test]
fn the_recovery_profile_exposes_its_six_tools() {
    let names = tool_names(&recovery_server(&desk()).catalog());
    assert_eq!(names.len(), 6);
    for tool in RECOVERY_TOOLS {
        assert!(names.iter().any(|name| name == tool), "missing {tool}");
    }
}

#[test]
fn host_selects_a_live_recovery_range_explicitly() {
    let options = parse_host_args(
        &[
            "--profile",
            "recovery",
            "--change",
            "checkout",
            "--base",
            "origin/main",
            "--head",
            "HEAD",
        ]
        .map(str::to_owned),
    )
    .unwrap();
    assert_eq!(options.profile, HostProfile::Recovery);
    assert_eq!(options.change, "checkout");
    assert_eq!(options.base, "origin/main");
    assert_eq!(options.head, "HEAD");
}

#[test]
fn the_default_profile_stays_at_seven_tools() {
    let service: Arc<dyn QualityService> = Arc::new(FakeService::default());
    let names = tool_names(&quality_server(&service).catalog());
    assert_eq!(
        names.len(),
        7,
        "recovery must not inflate the coding-agent surface"
    );
    for tool in RECOVERY_TOOLS {
        assert!(
            !names.iter().any(|name| name == tool),
            "{tool} leaked into the default profile"
        );
    }
}

#[test]
fn recovery_schemas_are_strict() {
    let built = recovery_server(&desk());
    assert!(
        built.schema_defects().is_empty(),
        "strict catalog defects: {:?}",
        built.schema_defects()
    );
}

#[test]
fn the_verify_tool_takes_exactly_one_candidate() {
    let catalog = recovery_server(&desk()).catalog();
    let tools = catalog.as_array().expect("catalog is an array");
    let verify = tools
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some("quality_spec_verify"))
        .expect("quality_spec_verify is advertised");
    let schema = verify
        .get("inputSchema")
        .or_else(|| verify.get("input_schema"))
        .expect("verify advertises an input schema");
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .expect("schema has properties");
    assert!(properties.contains_key("candidate"));
    assert!(
        !properties.contains_key("candidates"),
        "there is no bulk form"
    );
    assert_eq!(
        schema.get("additionalProperties").and_then(Value::as_bool),
        Some(false),
        "an accept-all flag must not even parse"
    );
}
