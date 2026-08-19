//! Task 16: fake-provider bus tests. No transport types.

use std::path::{Path, PathBuf};

use wvq_command_bus::{
    BusError, Command, ContextCommand, EvidenceCommand, ExplainCommand, FakeService, INLINE_LIMIT,
    LiveService, PlanCommand, QualityService, Reply, RunCommand, SelectCommand, SpecCommand,
    VerifyCommand, dispatch, estimate_tokens,
};

fn fixture_repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("openspec")
        .join("repo")
}

#[test]
fn context_reply_is_bounded_by_token_budget() {
    let fake = FakeService::default();
    fake.set_context_items(vec![
        "requirement alpha is a long neighbouring clause about overflow".into(),
        "requirement beta is another neighbouring clause about grouping".into(),
        "obligation others-visible".into(),
        "heuristic: 0 runtime LLM tokens".into(),
    ]);
    let reply = fake
        .context(&ContextCommand {
            change: "sankey-others".into(),
            purpose: "implementation".into(),
            token_budget: 8,
        })
        .unwrap();
    assert!(reply.truncated);
    assert!(reply.tokens_used <= 8);
    let total = reply.requirements.len()
        + reply.obligations.len()
        + reply.heuristics.len()
        + reply.coverage.len();
    assert!(total < 4, "budget must drop later items");
}

#[test]
fn unknown_purpose_fails_closed() {
    let fake = FakeService::default();
    let err = fake
        .context(&ContextCommand {
            change: "sankey-others".into(),
            purpose: "vibes".into(),
            token_budget: 4000,
        })
        .unwrap_err();
    assert!(matches!(
        err,
        BusError::Unknown {
            field: "purpose",
            ..
        }
    ));
}

#[test]
fn plan_does_not_execute() {
    let fake = FakeService::default();
    let reply = fake
        .plan(&PlanCommand {
            change: "sankey-others".into(),
        })
        .unwrap();
    assert!(!reply.executed);
    assert!(!fake.run_was_executed());
    let _ = fake
        .run(&RunCommand {
            change: "sankey-others".into(),
            scope: "impacted".into(),
            evidence_policy: "standard".into(),
        })
        .unwrap();
    assert!(fake.run_was_executed());
}

#[test]
fn unknown_scope_fails_closed() {
    let fake = FakeService::default();
    let err = fake
        .run(&RunCommand {
            change: "current".into(),
            scope: "everything-everywhere".into(),
            evidence_policy: "standard".into(),
        })
        .unwrap_err();
    assert!(matches!(err, BusError::Unknown { field: "scope", .. }));
}

#[test]
fn large_evidence_is_handle_only() {
    let fake = FakeService::default();
    let blob = vec![b'x'; INLINE_LIMIT + 16];
    fake.put_evidence("cas:big", blob);
    let reply = fake
        .evidence(&EvidenceCommand {
            handle: "cas:big".into(),
        })
        .unwrap();
    assert_eq!(reply.handle, "cas:big");
    assert!(reply.byte_len > INLINE_LIMIT as u64);
    assert!(reply.inline_text.is_none());
    assert!(reply.content_hash.is_some());
}

#[test]
fn small_utf8_evidence_is_inlined() {
    let fake = FakeService::default();
    fake.put_evidence("cas:tiny", b"junit snippet".to_vec());
    let reply = fake
        .evidence(&EvidenceCommand {
            handle: "cas:tiny".into(),
        })
        .unwrap();
    assert_eq!(reply.inline_text.as_deref(), Some("junit snippet"));
}

#[test]
fn contradicted_verify_is_blocking() {
    let fake = FakeService::default();
    fake.set_verdict("CONTRADICTED");
    let reply = dispatch(
        &fake,
        Command::Verify(VerifyCommand {
            change: "sankey-others".into(),
        }),
    )
    .unwrap();
    let Reply::Verify(body) = reply else {
        panic!("expected verify reply");
    };
    assert!(body.blocking);
    assert_eq!(body.exit_code(), 2);
    assert_eq!(body.verdict, "CONTRADICTED");
}

#[test]
fn proven_verify_is_zero_exit() {
    let fake = FakeService::default();
    fake.set_verdict("PROVEN");
    let reply = fake
        .verify(&VerifyCommand {
            change: "sankey-others".into(),
        })
        .unwrap();
    assert!(!reply.blocking);
    assert_eq!(reply.exit_code(), 0);
}

#[test]
fn explain_unknown_id_is_not_found() {
    let fake = FakeService::default();
    let err = fake
        .explain(&ExplainCommand {
            id: "missing".into(),
        })
        .unwrap_err();
    assert!(matches!(err, BusError::NotFound(_)));
}

#[test]
fn live_spec_validate_and_seal_sankey_others() {
    let service = LiveService::new(fixture_repo());
    let change = SpecCommand {
        change: "sankey-others".into(),
    };
    let valid = service.spec_validate(&change).unwrap();
    assert!(valid.ok);
    assert_eq!(valid.change, "sankey-others");
    assert!(valid.obligations >= 4);
    let sealed = service.spec_seal(&change).unwrap();
    assert!(sealed.seal_id.starts_with("oseal-"));
    assert_eq!(sealed.digest.len(), 64);
}

#[test]
fn live_current_change_is_ambiguous_in_fixture_repo() {
    let service = LiveService::new(fixture_repo());
    let err = service
        .spec_validate(&SpecCommand {
            change: "current".into(),
        })
        .unwrap_err();
    assert!(matches!(err, BusError::Ambiguous(_)));
}

#[test]
fn live_verify_without_runtime_is_unproven_not_failed() {
    let service = LiveService::new(fixture_repo());
    let reply = service
        .verify(&VerifyCommand {
            change: "sankey-others".into(),
        })
        .unwrap();
    assert_eq!(reply.verdict, "UNPROVEN");
    assert!(!reply.blocking);
    assert_eq!(reply.exit_code(), 1);
    assert!(reply.proofs.iter().all(|item| item.verdict == "UNPROVEN"));
}

#[test]
fn live_select_does_not_execute_and_keeps_mandatory_gaps() {
    let service = LiveService::new(fixture_repo());
    let reply = service
        .select(&SelectCommand {
            change: "sankey-others".into(),
        })
        .unwrap();
    assert!(!reply.executed);
    assert_eq!(reply.algorithm, "greedy-weighted-set-cover");
    assert!(reply.selected.is_empty());
    assert!(reply.uncovered_mandatory.contains(&"others-visible".into()));
}

#[test]
fn live_plan_sets_executed_false() {
    let service = LiveService::new(fixture_repo());
    let reply = service
        .plan(&PlanCommand {
            change: "sankey-others".into(),
        })
        .unwrap();
    assert!(!reply.executed);
    assert!(!reply.gaps.is_empty());
    assert!(reply.checks.contains(&"coverage".into()));
}

#[test]
fn estimate_tokens_is_stable() {
    assert_eq!(estimate_tokens(""), 0);
    assert_eq!(estimate_tokens("abcd"), 1);
    assert_eq!(estimate_tokens("abcdefgh"), 2);
}
