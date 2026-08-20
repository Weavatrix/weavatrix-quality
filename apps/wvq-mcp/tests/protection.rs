//! Task 35: the protection profile is separate from the default seven tools.

use std::sync::{Arc, Mutex};

use mcport::{ConcurrentToolServer, Value};
use wvq_command_bus::{FakeService, QualityService};
use wvq_mcp::{HostProfile, SharedProtection, parse_host_args, protection_server, quality_server};
use wvq_proof::{FlowView, ProtectionView, TestLineageView};

const PROTECTION_TOOLS: [&str; 3] = ["quality_protection", "quality_test_lineage", "quality_flow"];

fn view() -> SharedProtection {
    Arc::new(Mutex::new(ProtectionView {
        lineage: vec![TestLineageView {
            test: "auth-viewer.spec".into(),
            state: "unchanged".into(),
            matched_on: "test_name".into(),
            protection_changed: true,
            lost_flows: vec!["viewer-deny".into()],
            gained_flows: Vec::new(),
            phantom: true,
        }],
        flows: vec![FlowView {
            flow: "viewer-deny".into(),
            tests_before: vec!["auth-viewer.spec".into()],
            tests_after: Vec::new(),
            proof_before: vec!["P-811".into()],
            ..FlowView::default()
        }],
        ..ProtectionView::default()
    }))
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
fn the_protection_profile_exposes_its_three_tools() {
    let names = tool_names(&protection_server(&view()).catalog());
    assert_eq!(names.len(), 3);
    for tool in PROTECTION_TOOLS {
        assert!(names.iter().any(|name| name == tool), "missing {tool}");
    }
}

#[test]
fn host_selects_a_live_protection_range_explicitly() {
    let options = parse_host_args(
        &[
            "--profile",
            "protection",
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
    assert_eq!(options.profile, HostProfile::Protection);
    assert_eq!(options.change, "checkout");
    assert_eq!(options.base, "origin/main");
    assert_eq!(options.head, "HEAD");
}

#[test]
fn the_default_profile_stays_at_seven_tools() {
    let service: Arc<dyn QualityService> = Arc::new(FakeService::default());
    let names = tool_names(&quality_server(&service).catalog());
    assert_eq!(names.len(), 7);
    for tool in PROTECTION_TOOLS {
        assert!(
            !names.iter().any(|name| name == tool),
            "{tool} leaked into the coding-agent surface"
        );
    }
}

#[test]
fn protection_schemas_are_strict() {
    let built = protection_server(&view());
    assert!(
        built.schema_defects().is_empty(),
        "strict catalog defects: {:?}",
        built.schema_defects()
    );
}

#[test]
fn the_view_answers_what_protected_this_before() {
    let shared = view();
    let guard = shared.lock().expect("view");

    let lineage = guard
        .lineage_of("auth-viewer.spec")
        .expect("lineage is recorded");
    assert!(
        lineage.phantom,
        "a green test that stopped guarding must be visible on the lineage screen"
    );
    assert_eq!(lineage.lost_flows, vec!["viewer-deny"]);

    let flow = guard.flow("viewer-deny").expect("flow is recorded");
    assert_eq!(flow.tests_before, vec!["auth-viewer.spec"]);
    assert!(
        flow.tests_after.is_empty(),
        "QA can see in one view that nothing protects it now"
    );
    assert_eq!(flow.proof_before, vec!["P-811"]);
}

#[test]
fn the_report_hides_healthy_flows() {
    let guard = view();
    let report = guard.lock().expect("view").report();
    assert!(report.needs_attention.is_empty());
    assert_eq!(report.suppressed_healthy, 0);
    assert!(!report.blocking);
}
