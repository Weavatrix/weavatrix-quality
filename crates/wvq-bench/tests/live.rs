use wvq_bench::run_live_shadow;
use wvq_command_bus::FakeService;

#[test]
fn live_shadow_executes_impacted_and_all_and_measures_stored_evidence() {
    let service = FakeService::default();
    service.put_evidence(
        "execution-summary",
        serde_json::to_vec(&serde_json::json!({
            "schema_v": 1,
            "executors": [{"executor": "vitest"}, {"executor": "playwright"}],
            "browser_programs": [{"program": "overflow"}],
        }))
        .unwrap(),
    );
    service.put_evidence("screenshot", vec![0_u8; 17]);

    let report = run_live_shadow(
        &service,
        "example-change",
        "base-sha",
        "WORKTREE",
        "minimal",
    )
    .unwrap();

    assert_eq!(report.measurement_kind, "live_execution");
    assert_eq!(report.impacted.requested_scope, "impacted");
    assert_eq!(report.full.requested_scope, "all");
    assert_eq!(
        report.impacted.scope_reason,
        "impacted scope requested by caller"
    );
    assert_eq!(report.impacted.outcome, "passed");
    assert_eq!(report.full.outcome, "passed");
    assert_eq!(report.impacted.executor_invocations, Some(2));
    assert_eq!(report.impacted.browser_programs, Some(1));
    assert_eq!(report.impacted.artifact_count, 2);
    assert_eq!(report.impacted.artifact_handles.len(), 2);
    assert_eq!(report.impacted.artifact_bytes, report.full.artifact_bytes);
    assert_eq!(report.runtime_llm_tokens, 0);
    assert!(report.comparable);
}

#[test]
fn live_shadow_refuses_unknown_evidence_policy_before_execution() {
    let service = FakeService::default();
    let error = run_live_shadow(
        &service,
        "example-change",
        "base-sha",
        "WORKTREE",
        "everything",
    )
    .unwrap_err();
    assert!(error.to_string().contains("evidence policy"));
    assert!(!service.run_was_executed());
}
