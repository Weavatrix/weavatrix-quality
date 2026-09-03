//! `wvq doctor` discovers runners and policy without writing or sealing.

use super::*;
use crate::{DoctorCommand, InitCommand};

#[test]
fn doctor_on_an_empty_repo_is_not_authority_and_suggests_init() {
    let root = TempDir::new("wvq-doctor-empty");
    let reply = LiveService::new(&root.0)
        .doctor(&DoctorCommand::default())
        .unwrap();
    assert!(!reply.authority);
    assert!(!reply.policy_present);
    assert!(!reply.policy_loadable);
    assert!(!reply.openspec_present);
    assert!(reply.runners.is_empty());
    assert!(reply.bindings.is_empty());
    assert_eq!(reply.runtime_llm_tokens, 0);
    assert!(reply.suggested_next.iter().any(|item| item == "wvq init"));
    assert!(!root.0.join(".weavatrix-quality").exists());
}

#[test]
fn doctor_does_not_invent_bindings_after_init() {
    let root = TempDir::new("wvq-doctor-init");
    let service = LiveService::new(&root.0);
    service.init(&InitCommand { force: false }).unwrap();
    let before = std::fs::read_to_string(root.0.join(".weavatrix-quality/config.yaml")).unwrap();
    let reply = service.doctor(&DoctorCommand::default()).unwrap();
    assert!(!reply.authority);
    assert!(reply.policy_present);
    assert!(reply.policy_loadable);
    assert!(reply.bindings.is_empty());
    assert!(!reply.browser_configured);
    assert!(
        reply
            .suggested_next
            .iter()
            .any(|item| item.contains("OpenSpec"))
    );
    let after = std::fs::read_to_string(root.0.join(".weavatrix-quality/config.yaml")).unwrap();
    assert_eq!(before, after);
}

#[test]
fn doctor_names_a_go_executor_without_sealing_it() {
    let root = TempDir::new("wvq-doctor-go");
    std::fs::write(
        root.0.join("go.mod"),
        "module fixture.local/doctor\n\ngo 1.24\n",
    )
    .unwrap();
    let reply = LiveService::new(&root.0)
        .doctor(&DoctorCommand::default())
        .unwrap();
    assert!(reply.ecosystems.iter().any(|item| item == "go"));
    assert_eq!(reply.runners[0].executor, "go-test");
    assert_eq!(reply.runners[0].cwd, ".");
    assert!(reply.bindings.is_empty());
}

#[test]
fn doctor_lists_openspec_change_folders() {
    let root = TempDir::new("wvq-doctor-spec");
    std::fs::create_dir_all(root.0.join("openspec/changes/limit-change")).unwrap();
    let reply = LiveService::new(&root.0)
        .doctor(&DoctorCommand::default())
        .unwrap();
    assert!(reply.openspec_present);
    assert_eq!(reply.openspec_changes, ["limit-change"]);
}

#[test]
fn doctor_reports_a_malformed_policy_instead_of_inventing_a_fix() {
    let root = TempDir::new("wvq-doctor-bad-policy");
    std::fs::create_dir_all(root.0.join(".weavatrix-quality")).unwrap();
    std::fs::write(
        root.0.join(".weavatrix-quality/config.yaml"),
        "quality_policy_v: 99\n",
    )
    .unwrap();
    let reply = LiveService::new(&root.0)
        .doctor(&DoctorCommand::default())
        .unwrap();
    assert!(reply.policy_present);
    assert!(!reply.policy_loadable);
    assert!(
        reply
            .policy_error
            .as_deref()
            .is_some_and(|error| error.contains("unknown quality_policy_v 99"))
    );
}

#[test]
fn doctor_rejects_a_missing_directory() {
    let missing = std::env::temp_dir().join(format!(
        "wvq-doctor-missing-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let err = LiveService::new(&missing)
        .doctor(&DoctorCommand::default())
        .unwrap_err();
    assert!(err.to_string().contains("requires a directory"), "{err}");
}
