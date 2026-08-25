//! Task 18: `TestProgram` IR, fail-closed protocol, evidence-policy screenshots.

use std::path::{Path, PathBuf};

use wvq_runtime::{
    BridgeRequest, CaptureWhen, Observation, ProgramError, TestAction, TestProgram, decode_request,
    filter_observation,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("browser")
        .join(name)
}

fn read(name: &str) -> String {
    std::fs::read_to_string(fixture(name)).unwrap()
}

#[test]
fn valid_program_loads_with_seed_and_obligations() {
    let program = TestProgram::from_json(&read("program.valid.json")).unwrap();
    assert_eq!(program.schema_v, 1);
    assert_eq!(program.id.as_str(), "sankey-others-visible");
    assert_eq!(program.deterministic_seed, Some(7));
    assert_eq!(program.obligations.len(), 1);
    assert_eq!(program.steps.len(), 3);
    assert!(matches!(program.steps[0], TestAction::Navigate { .. }));
}

#[test]
fn xpath_target_is_rejected() {
    let err = TestProgram::from_json(&read("program.xpath.json")).unwrap_err();
    assert!(
        matches!(err, ProgramError::Invalid(ref message) if message.contains("XPath"))
            || matches!(err, ProgramError::Malformed(_))
    );
}

#[test]
fn unknown_action_is_rejected() {
    let raw = r#"{
        "schema_v": 1,
        "id": "p1",
        "source": "authored",
        "obligations": ["others-visible"],
        "steps": [{ "action": "browser_magic", "target": { "test_id": "x" } }]
    }"#;
    let err = TestProgram::from_json(raw).unwrap_err();
    assert!(matches!(err, ProgramError::Malformed(_)));
}

#[test]
fn hover_scroll_and_drag_are_first_class_actions() {
    let program = TestProgram::from_json(
        r#"{
        "schema_v": 1,
        "id": "pointer-actions",
        "source": "authored",
        "obligations": ["menu-open"],
        "steps": [
            { "action": "hover", "target": { "role": "button", "accessible_name": "File" } },
            { "action": "scroll", "target": { "test_id": "footer" } },
            {
                "action": "drag",
                "target": { "test_id": "chip" },
                "to": { "test_id": "tray" }
            },
            { "action": "assert", "obligation": "menu-open" }
        ]
    }"#,
    )
    .unwrap();
    assert!(matches!(program.steps[0], TestAction::Hover { .. }));
    assert!(matches!(program.steps[1], TestAction::Scroll { .. }));
    assert!(matches!(program.steps[2], TestAction::Drag { .. }));
    assert_eq!(program.steps[0].kind(), "hover");
    assert_eq!(
        program.steps[0].semantic_target().unwrap().accessible_name.as_deref(),
        Some("File")
    );
}

#[test]
fn drag_without_a_drop_target_is_rejected() {
    let err = TestProgram::from_json(
        r#"{
        "schema_v": 1,
        "id": "p1",
        "source": "authored",
        "obligations": ["moved"],
        "steps": [{ "action": "drag", "target": { "test_id": "chip" } }]
    }"#,
    )
    .unwrap_err();
    assert!(matches!(err, ProgramError::Malformed(_)));
}

#[test]
fn empty_target_is_rejected() {
    let raw = r#"{
        "schema_v": 1,
        "id": "p1",
        "source": "authored",
        "obligations": ["others-visible"],
        "steps": [{ "action": "activate", "target": {} }]
    }"#;
    let err = TestProgram::from_json(raw).unwrap_err();
    assert!(matches!(err, ProgramError::Invalid(_)));
}

#[test]
fn blank_and_xpath_fallback_targets_are_rejected() {
    for target in [r#"{"label":"  "}"#, r#"{"fallback_css":"//button"}"#] {
        let raw = format!(
            r#"{{
                "schema_v": 1,
                "id": "p1",
                "source": "authored",
                "obligations": ["others-visible"],
                "steps": [
                    {{ "action": "activate", "target": {target} }},
                    {{ "action": "assert", "obligation": "others-visible" }}
                ]
            }}"#
        );
        let err = TestProgram::from_json(&raw).unwrap_err();
        assert!(matches!(err, ProgramError::Invalid(_)), "{err}");
    }
}

#[test]
fn every_declared_obligation_must_be_asserted() {
    let raw = r#"{
        "schema_v": 1,
        "id": "p1",
        "source": "authored",
        "obligations": ["others-visible"],
        "steps": [{ "action": "navigate", "route": "/" }]
    }"#;
    let err = TestProgram::from_json(raw).unwrap_err();
    assert!(matches!(err, ProgramError::Invalid(message) if message.contains("never asserted")));
}

#[test]
fn runtime_actions_must_reference_registered_resources() {
    for action in [
        r#"{"action":"inject_fault","fault":"missing"}"#,
        r#"{"action":"api_call","operation":"missing","input":"missing"}"#,
    ] {
        let raw = format!(
            r#"{{
                "schema_v": 1,
                "id": "p1",
                "source": "authored",
                "obligations": ["others-visible"],
                "steps": [
                    {action},
                    {{ "action": "assert", "obligation": "others-visible" }}
                ]
            }}"#
        );
        let err = TestProgram::from_json(&raw).unwrap_err();
        assert!(matches!(err, ProgramError::Invalid(_)), "{err}");
    }
}

#[test]
fn assertions_cannot_hide_in_preconditions() {
    let raw = r#"{
        "schema_v": 1,
        "id": "p1",
        "source": "authored",
        "obligations": ["others-visible"],
        "preconditions": [{ "action": "assert", "obligation": "others-visible" }],
        "steps": [{ "action": "navigate", "route": "/" }]
    }"#;
    let err = TestProgram::from_json(raw).unwrap_err();
    assert!(matches!(err, ProgramError::Invalid(message) if message.contains("preconditions")));
}

#[test]
fn unknown_schema_fails_closed() {
    let raw = r#"{
        "schema_v": 2,
        "id": "p1",
        "source": "authored",
        "obligations": ["others-visible"],
        "steps": [{ "action": "navigate", "route": "/" }]
    }"#;
    let err = TestProgram::from_json(raw).unwrap_err();
    assert!(matches!(err, ProgramError::UnknownSchema(2)));
}

#[test]
fn initialize_and_unknown_method_from_golden_fixtures() {
    let init = decode_request(read("protocol.initialize.json").trim()).unwrap();
    assert!(matches!(
        init,
        BridgeRequest::Initialize { id: 1, schema_v: 1 }
    ));
    let err = decode_request(read("protocol.unknown.json").trim()).unwrap_err();
    assert!(matches!(err, ProgramError::Invalid(message) if message.contains("browser_click")));
}

#[test]
fn screenshot_only_when_policy_allows() {
    let mut raw = Observation {
        screenshot_handle: Some("cas:shot".into()),
        network: vec!["GET /api".into()],
        ..Observation::default()
    };
    let policy = wvq_runtime::EvidencePolicy {
        screenshot: CaptureWhen::Never,
        ..wvq_runtime::EvidencePolicy::default()
    };
    raw.visual_digest = Some("deadbeef".into());
    raw.visual_surface = Some("screenshot_png".into());
    let filtered = filter_observation(raw.clone(), &policy, true);
    assert!(filtered.screenshot_handle.is_none());
    assert!(filtered.visual_digest.is_none());
    assert!(filtered.visual_surface.is_none());

    let policy = wvq_runtime::EvidencePolicy {
        screenshot: CaptureWhen::OnFailure,
        ..wvq_runtime::EvidencePolicy::default()
    };
    raw.screenshot_handle = Some("cas:shot".into());
    let on_pass = filter_observation(raw.clone(), &policy, false);
    assert!(on_pass.screenshot_handle.is_none());
    assert!(on_pass.visual_digest.is_none());
    let on_fail = filter_observation(raw, &policy, true);
    assert_eq!(on_fail.screenshot_handle.as_deref(), Some("cas:shot"));
    assert_eq!(on_fail.visual_digest.as_deref(), Some("deadbeef"));
    assert_eq!(on_fail.visual_surface.as_deref(), Some("screenshot_png"));
}

#[test]
fn ts_bridge_has_no_ai_logic() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("js")
        .join("playwright-runner")
        .join("src");
    let mut combined = String::new();
    // Every file the bridge ships, including the UI-integrity collector: it
    // measures geometry and must never grow a model call either.
    for name in [
        "main.ts",
        "protocol.ts",
        "execute.ts",
        "observe.ts",
        "record.ts",
        "playwright.ts",
        "ui_integrity.ts",
    ] {
        combined.push_str(&std::fs::read_to_string(dir.join(name)).unwrap());
    }
    let lower = combined.to_ascii_lowercase();
    for banned in [
        "openai",
        "anthropic",
        "llm",
        "chatgpt",
        "completion",
        "vision_call",
    ] {
        assert!(
            !lower.contains(banned),
            "Playwright bridge must not contain AI logic ({banned})"
        );
    }
}
