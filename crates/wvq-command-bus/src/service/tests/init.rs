//! `wvq init` writes a fail-closed policy and never invents bindings.

use super::*;
use crate::InitCommand;

#[test]
fn init_writes_a_loadable_policy_and_refuses_to_clobber_it() {
    let root = TempDir::new("wvq-init");
    let service = LiveService::new(&root.0);

    let first = service.init(&InitCommand { force: false }).unwrap();
    assert_eq!(first.runtime_llm_tokens, 0);
    assert_eq!(
        first.created,
        [
            ".weavatrix-quality/config.yaml",
            ".weavatrix-quality/.gitignore"
        ]
    );
    assert!(first.skipped.is_empty());
    assert!(!root.0.join(".weavatrix-quality/quality.db").exists());

    let raw = std::fs::read_to_string(root.0.join(".weavatrix-quality/config.yaml")).unwrap();
    assert!(raw.contains("quality_policy_v: 1"));
    assert!(raw.contains("no_new_debt"));
    assert!(raw.contains("ui_integrity:"));
    assert!(load_test_bindings(&root.0).unwrap().is_empty());
    assert!(load_debt_exceptions(&root.0).unwrap().active.is_empty());
    let ui = load_ui_integrity_policy(&root.0).unwrap();
    assert!(ui.enabled);
    let ignore = std::fs::read_to_string(root.0.join(".weavatrix-quality/.gitignore")).unwrap();
    assert!(ignore.contains("quality.db"));

    let err = service.init(&InitCommand { force: false }).unwrap_err();
    assert!(
        err.to_string().contains("already exists"),
        "{err}"
    );

    std::fs::write(
        root.0.join(".weavatrix-quality/config.yaml"),
        "quality_policy_v: 1\n",
    )
    .unwrap();
    let forced = service.init(&InitCommand { force: true }).unwrap();
    assert!(forced.created.contains(&".weavatrix-quality/config.yaml".into()));
    let rewritten = std::fs::read_to_string(root.0.join(".weavatrix-quality/config.yaml")).unwrap();
    assert!(rewritten.contains("no_new_debt"));
    assert!(rewritten.contains("ui_integrity:"));
}

#[test]
fn init_does_not_invent_a_browser_or_model_endpoint() {
    let root = TempDir::new("wvq-init-absent");
    LiveService::new(&root.0)
        .init(&InitCommand { force: false })
        .unwrap();
    let raw = std::fs::read_to_string(root.0.join(".weavatrix-quality/config.yaml")).unwrap();
    assert!(!raw.contains("base_url"));
    assert!(!raw.contains("endpoint"));
    assert!(!raw.contains("test_bindings"));
}

#[test]
fn init_rejects_a_missing_directory() {
    let missing = std::env::temp_dir().join(format!(
        "wvq-init-missing-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let err = LiveService::new(&missing)
        .init(&InitCommand { force: false })
        .unwrap_err();
    assert!(err.to_string().contains("requires a directory"), "{err}");
}
