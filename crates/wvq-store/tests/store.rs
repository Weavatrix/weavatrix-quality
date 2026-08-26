//! Task 14: CAS dedup, rollback leaves no artifact ref, proofs cannot change.

use std::time::{SystemTime, UNIX_EPOCH};

use wvq_domain::{
    ArtifactId, HumanDecision, HumanDecisionId, HumanRole, NewDecision, ObligationId, OracleSealId,
    ProofId, RevisionId, VerificationDecision,
};
use wvq_store::{
    Store, StoredAiUsage, StoredProof, StoredRun, StoredRunItem, StoredSelectionAudit,
    StoredTestCaseResult,
};

fn open_temp() -> Store {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("wvq-store-{nanos}"));
    std::fs::create_dir_all(&root).expect("temp dir");
    Store::open(&root).expect("open store")
}

#[test]
fn schema_version_is_recorded() {
    let store = open_temp();
    assert_eq!(store.schema_version().unwrap(), 13);
}

#[test]
fn defensive_selection_miss_is_queryable_and_idempotent() {
    let store = open_temp();
    let revision = RevisionId::new("rev-selection-audit").unwrap();
    let impacted_run = wvq_domain::RunId::new("run-selection-impacted").unwrap();
    let full_run = wvq_domain::RunId::new("run-selection-full").unwrap();
    for (run_id, outcome) in [(&impacted_run, "passed"), (&full_run, "failed")] {
        store
            .put_run(&StoredRun {
                id: run_id.clone(),
                change_id: "selection-audit".into(),
                revision: revision.clone(),
                status: "complete".into(),
                passed: outcome == "passed",
                outcome: outcome.into(),
            })
            .unwrap();
    }
    store
        .put_selection_audit(&StoredSelectionAudit {
            id: "selection-audit-1".into(),
            impacted_run: impacted_run.clone(),
            full_run,
            change_id: "selection-audit".into(),
            revision: revision.clone(),
            status: "contradicted".into(),
            missed_failures: 1,
            learned_tests: 1,
        })
        .unwrap();
    for _ in 0..2 {
        store
            .observe_selection_miss(
                "selection-audit-1",
                "tests/cart.test.ts",
                &["symbol:cart".into()],
                &revision,
            )
            .unwrap();
    }

    let learned = store
        .historical_tests_for_nodes(&["symbol:cart".into()], 2, 100)
        .unwrap();
    assert_eq!(learned.len(), 1);
    assert_eq!(learned[0].test_path, "tests/cart.test.ts");
    assert_eq!(learned[0].minimum_observations, 0);
    assert_eq!(learned[0].defensive_misses, 1);
}

#[test]
fn historical_test_node_candidates_require_repeated_exact_observations() {
    let store = open_temp();
    let first = RevisionId::new("rev-history-1").unwrap();
    let second = RevisionId::new("rev-history-2").unwrap();
    let first_run = wvq_domain::RunId::new("run-history-1").unwrap();
    let second_run = wvq_domain::RunId::new("run-history-2").unwrap();
    for (run_id, revision) in [(&first_run, &first), (&second_run, &second)] {
        store
            .put_run(&StoredRun {
                id: run_id.clone(),
                change_id: "selection-history".into(),
                revision: revision.clone(),
                status: "complete".into(),
                passed: true,
                outcome: "passed".into(),
            })
            .unwrap();
    }
    store
        .observe_test_nodes(
            &first_run,
            "tests/cart.test.ts",
            &["symbol:cart".into(), "symbol:voucher".into()],
            &first,
        )
        .unwrap();
    assert!(
        store
            .historical_tests_for_nodes(&["symbol:cart".into()], 2, 100)
            .unwrap()
            .is_empty(),
        "one observation is not confident enough to affect selection"
    );
    store
        .observe_test_nodes(
            &second_run,
            "tests/cart.test.ts",
            &["symbol:cart".into()],
            &second,
        )
        .unwrap();

    let learned = store
        .historical_tests_for_nodes(&["symbol:cart".into(), "symbol:unrelated".into()], 2, 100)
        .unwrap();
    assert_eq!(learned.len(), 1);
    assert_eq!(learned[0].test_path, "tests/cart.test.ts");
    assert_eq!(learned[0].matched_nodes, ["symbol:cart"]);
    assert_eq!(learned[0].minimum_observations, 2);
    assert_eq!(learned[0].last_revision, second);
}

#[test]
fn test_case_history_exposes_duration_and_real_flakiness() {
    let store = open_temp();
    let revision = RevisionId::new("rev-test-analytics").unwrap();
    for (run, status, duration_ms) in [
        ("run-test-analytics-1", "pass", 20),
        ("run-test-analytics-2", "fail", 40),
    ] {
        let run_id = wvq_domain::RunId::new(run).unwrap();
        store
            .put_run(&StoredRun {
                id: run_id.clone(),
                change_id: "test-analytics".into(),
                revision: revision.clone(),
                status: "complete".into(),
                passed: status == "pass",
                outcome: if status == "pass" { "passed" } else { "failed" }.into(),
            })
            .unwrap();
        store
            .put_test_case_result(&StoredTestCaseResult {
                id: format!("{run}-case-1"),
                run_id,
                revision: revision.clone(),
                executor: "vitest".into(),
                suite: "src/cart.test.ts".into(),
                name: "preserves an applied voucher".into(),
                status: status.into(),
                duration_ms: Some(duration_ms),
                fingerprint: None,
            })
            .unwrap();
    }

    let stats = store
        .test_case_stats("vitest", "src/cart.test.ts", "preserves an applied voucher")
        .unwrap();
    assert_eq!(stats.runs, 2);
    assert_eq!(stats.passes, 1);
    assert_eq!(stats.failures, 1);
    assert_eq!(stats.errors, 0);
    assert_eq!(stats.skips, 0);
    assert_eq!(stats.average_duration_ms, Some(30));
    assert!(stats.flaky);
}

#[test]
fn runs_survive_process_boundaries_with_artifact_handles() {
    let store = open_temp();
    let run_id = wvq_domain::RunId::new("run-live-1").unwrap();
    let revision = RevisionId::new("rev-live-1").unwrap();
    let artifact = ArtifactId::new("artifact-run-live-1-summary").unwrap();
    store
        .put_artifact(&artifact, "execution-summary", br#"{"passed":true}"#)
        .unwrap();
    store
        .put_run(&StoredRun {
            id: run_id.clone(),
            change_id: "sankey-others".into(),
            revision: revision.clone(),
            status: "complete".into(),
            passed: true,
            outcome: "passed".into(),
        })
        .unwrap();
    store
        .put_run_item(&StoredRunItem {
            id: "run-live-1-cargo-test".into(),
            run_id: run_id.clone(),
            executor: "cargo-test".into(),
            status_code: Some(0),
            passed: true,
        })
        .unwrap();
    store.attach_run_artifact(&run_id, &artifact).unwrap();

    let loaded = store
        .latest_run("sankey-others", &revision)
        .unwrap()
        .expect("run");
    assert_eq!(
        loaded,
        StoredRun {
            id: run_id.clone(),
            change_id: "sankey-others".into(),
            revision,
            status: "complete".into(),
            passed: true,
            outcome: "passed".into(),
        }
    );
    assert_eq!(store.run_artifacts(&run_id).unwrap(), [artifact]);
}

#[test]
fn proof_artifact_provenance_survives_process_boundaries() {
    let store = open_temp();
    let proof = StoredProof {
        id: ProofId::new("proof-provenance-1").unwrap(),
        revision: RevisionId::new("rev-provenance-1").unwrap(),
        obligation: ObligationId::new("obligation-provenance-1").unwrap(),
        oracle_seal: OracleSealId::new("seal-provenance-1").unwrap(),
        verdict: "PROVEN".into(),
    };
    let artifact = ArtifactId::new("artifact-proof-provenance-1").unwrap();
    store
        .put_artifact(&artifact, "revision-range", br#"{"schema_v":2}"#)
        .unwrap();
    store
        .put_proof_with_artifacts(&proof, std::slice::from_ref(&artifact))
        .unwrap();

    assert_eq!(store.proof_artifacts(&proof.id).unwrap(), [artifact]);
}

#[test]
fn content_hash_deduplicates_cas_objects() {
    let store = open_temp();
    let a = store.put_blob(b"same-bytes").unwrap();
    let b = store.put_blob(b"same-bytes").unwrap();
    assert_eq!(a, b);
    assert_eq!(store.cas().object_count().unwrap(), 1);
    let other = store.put_blob(b"other").unwrap();
    assert_ne!(a, other);
    assert_eq!(store.cas().object_count().unwrap(), 2);
}

#[test]
fn blobs_are_not_stored_in_sqlite() {
    let store = open_temp();
    let id = ArtifactId::new("art-1").unwrap();
    let hash = store.put_artifact(&id, "junit", b"not-in-sqlite").unwrap();
    let rec = store.get_artifact(&id).unwrap().expect("row");
    assert_eq!(rec.content_hash, hash);
    let on_disk = store.cas().get(&hash).unwrap();
    assert_eq!(on_disk, b"not-in-sqlite");
}

#[test]
fn transaction_rollback_drops_artifact_ref() {
    let mut store = open_temp();
    let id = ArtifactId::new("art-rollback").unwrap();
    let hash = store.put_blob(b"blob").unwrap();
    {
        let tx = store.transaction().unwrap();
        Store::insert_artifact_row(&tx, &id, "lcov", &hash, 4).unwrap();
        tx.rollback().unwrap();
    }
    assert!(store.get_artifact(&id).unwrap().is_none());
}

#[test]
fn proofs_are_immutable() {
    let store = open_temp();
    let proof = StoredProof {
        id: ProofId::new("proof-1").unwrap(),
        revision: RevisionId::new("rev-head").unwrap(),
        obligation: ObligationId::new("others-visible").unwrap(),
        oracle_seal: OracleSealId::new("oseal-abc").unwrap(),
        verdict: "PROVEN".into(),
    };
    store.put_proof(&proof).unwrap();
    let err = store
        .update_proof_verdict(&proof.id, "CONTRADICTED")
        .unwrap_err();
    assert!(matches!(err, wvq_store::StoreError::ProofImmutable));
    let loaded = store.get_proof(&proof.id).unwrap().expect("proof");
    assert_eq!(loaded.verdict, "PROVEN");
    let by_obligation = store
        .proof_for_obligation(&proof.revision, &proof.obligation)
        .unwrap()
        .expect("proof by revision and obligation");
    assert_eq!(by_obligation.id, proof.id);
    assert!(
        store
            .proof_for_obligation(&RevisionId::new("rev-other").unwrap(), &proof.obligation,)
            .unwrap()
            .is_none()
    );
}

#[test]
fn behavior_states_and_edges_persist() {
    let store = open_temp();
    let body = br#"{"route":"/analytics/dashboard/42"}"#;
    let digest = store.put_blob(body).unwrap();
    assert!(store.put_behavior_state(&digest, body).unwrap());
    assert!(!store.put_behavior_state(&digest, body).unwrap());
    assert_eq!(store.behavior_state_count().unwrap(), 1);
    assert!(store.has_behavior_state(&digest).unwrap());

    let after = store
        .put_blob(br#"{"route":"/analytics/dashboard/42","modal":"others"}"#)
        .unwrap();
    assert!(
        store
            .put_behavior_edge(&digest, &after, "activate")
            .unwrap()
    );
    assert!(
        !store
            .put_behavior_edge(&digest, &after, "activate")
            .unwrap()
    );
    assert_eq!(store.behavior_edge_count().unwrap(), 1);
    assert!(
        store
            .has_behavior_edge(&digest, &after, "activate")
            .unwrap()
    );

    store
        .put_manual_session("sess-1", Some(7), Some("admin-above-limit"))
        .unwrap();
    let session = store
        .get_manual_session("sess-1")
        .unwrap()
        .expect("session");
    assert_eq!(session.seed, Some(7));
    assert_eq!(session.fixture.as_deref(), Some("admin-above-limit"));
    assert_eq!(session.repository_revision, "");

    let trace = br#"{"session_id":"sess-passive"}"#;
    let trace_hash = store
        .put_recorded_session(
            "sess-passive",
            Some(9),
            Some("safe-fixture"),
            "rev-recorded",
            Some("preview-recorded"),
            trace,
            &[("{\"action\":\"activate\"}".into(), after.clone())],
            &["details-visible".into()],
            &["GET /api/details".into()],
        )
        .unwrap();
    let passive = store
        .get_manual_session("sess-passive")
        .unwrap()
        .expect("passive session");
    assert_eq!(passive.repository_revision, "rev-recorded");
    assert_eq!(passive.trace_hash.as_deref(), Some(trace_hash.as_str()));
    assert_eq!(passive.preview_id.as_deref(), Some("preview-recorded"));
    assert!(store.has_behavior_obligation("details-visible").unwrap());
    assert!(
        store
            .has_behavior_api_operation("GET /api/details")
            .unwrap()
    );
}

#[test]
fn failure_fingerprints_cluster_occurrences() {
    let store = open_temp();
    let digest = store.put_blob(b"flake-identity").unwrap();
    store.put_failure_fingerprint(&digest, "timing").unwrap();
    store.put_failure_occurrence("occ-1", &digest).unwrap();
    store.put_failure_occurrence("occ-2", &digest).unwrap();
    assert_eq!(store.failure_cluster_size(&digest).unwrap(), 2);
    store
        .put_program_revision("sankey-others-replay", 2, "oseal-abc")
        .unwrap();
    assert_eq!(
        store
            .latest_program_revision("sankey-others-replay")
            .unwrap(),
        Some(2)
    );
}

#[test]
fn passing_authoring_preview_promotes_once_with_canonical_body() {
    let mut store = open_temp();
    let body = br#"{"schema_v":1,"id":"generated-browser"}"#;
    store
        .put_authoring_preview(
            "preview-generated-browser",
            "generated-browser",
            "live-change",
            "rev-head",
            "oseal-abc",
            true,
            body,
        )
        .unwrap();

    assert_eq!(
        store
            .promote_authoring_preview(
                "preview-generated-browser",
                "generated-browser",
                "live-change",
                "rev-head",
                "oseal-abc",
                body,
            )
            .unwrap(),
        (1, true)
    );
    assert_eq!(
        store
            .promote_authoring_preview(
                "preview-generated-browser",
                "generated-browser",
                "live-change",
                "rev-head",
                "oseal-abc",
                body,
            )
            .unwrap(),
        (1, false)
    );
    let (record, stored_body) = store
        .read_program_revision("generated-browser", 1)
        .unwrap()
        .expect("promoted program");
    assert_eq!(record.source, "promoted");
    assert_eq!(
        record.preview_id.as_deref(),
        Some("preview-generated-browser")
    );
    assert_eq!(stored_body, body);
    let latest = store
        .latest_program_revisions_for_change("live-change")
        .unwrap();
    assert_eq!(latest.len(), 1);
    assert_eq!(latest[0].0.program, "generated-browser");
    assert_eq!(latest[0].1, body);

    store
        .put_authoring_preview(
            "preview-failed-browser",
            "failed-browser",
            "live-change",
            "rev-head",
            "oseal-abc",
            false,
            body,
        )
        .unwrap();
    assert!(
        store
            .promote_authoring_preview(
                "preview-failed-browser",
                "failed-browser",
                "live-change",
                "rev-head",
                "oseal-abc",
                body,
            )
            .is_err()
    );
}

#[test]
fn passing_healing_preview_appends_one_same_seal_revision() {
    let mut store = open_temp();
    let original = br#"{"schema_v":1,"id":"heal-browser","steps":[]}"#;
    store
        .put_authoring_preview(
            "preview-heal-original",
            "heal-browser",
            "live-change",
            "rev-one",
            "oseal-abc",
            true,
            original,
        )
        .unwrap();
    store
        .promote_authoring_preview(
            "preview-heal-original",
            "heal-browser",
            "live-change",
            "rev-one",
            "oseal-abc",
            original,
        )
        .unwrap();

    let healed = br#"{"schema_v":1,"id":"heal-browser","steps":[{"action":"wait"}]}"#;
    store
        .put_authoring_preview(
            "preview-heal-two",
            "heal-browser",
            "live-change",
            "rev-two",
            "oseal-abc",
            true,
            healed,
        )
        .unwrap();
    assert_eq!(
        store
            .heal_authoring_preview(
                "preview-heal-two",
                "heal-browser",
                1,
                "live-change",
                "rev-two",
                "oseal-abc",
                healed,
            )
            .unwrap(),
        (2, true)
    );
    assert_eq!(
        store
            .heal_authoring_preview(
                "preview-heal-two",
                "heal-browser",
                1,
                "live-change",
                "rev-two",
                "oseal-abc",
                healed,
            )
            .unwrap(),
        (2, false)
    );
    let (record, body) = store
        .read_program_revision("heal-browser", 2)
        .unwrap()
        .expect("healed revision");
    assert_eq!(record.source, "healed");
    assert_eq!(record.seal, "oseal-abc");
    assert_eq!(body, healed);
}

#[test]
fn mutation_results_persist() {
    let store = open_temp();
    store
        .put_mutation_result("ts-0-src/add.ts:1", "CmpGtGe", "src/add.ts:1", "survived")
        .unwrap();
    assert_eq!(
        store
            .mutation_status("ts-0-src/add.ts:1")
            .unwrap()
            .as_deref(),
        Some("survived")
    );
    assert_eq!(store.mutation_status("ts-0-absent").unwrap(), None);
}

#[test]
fn ai_budget_persists_per_change_and_run() {
    let store = open_temp();
    assert_eq!(store.ai_usage_for_change("sankey-others").unwrap(), None);
    store
        .put_ai_usage(
            "ai-plan",
            &StoredAiUsage {
                change_id: "sankey-others".into(),
                run_id: None,
                planning_tokens: 1_800,
                cost_micros: 120,
                ..StoredAiUsage::default()
            },
        )
        .unwrap();
    store
        .put_ai_usage(
            "ai-run-1",
            &StoredAiUsage {
                change_id: "sankey-others".into(),
                run_id: Some("run-1".into()),
                planning_tokens: 200,
                runtime_tokens: 40,
                browser_escape_calls: 1,
                vision_calls: 1,
                cost_micros: 80,
            },
        )
        .unwrap();
    let total = store
        .ai_usage_for_change("sankey-others")
        .unwrap()
        .expect("usage was recorded");
    assert_eq!(total.planning_tokens, 2_000);
    assert_eq!(total.runtime_tokens, 40);
    assert_eq!(total.browser_escape_calls, 1);
    assert_eq!(total.vision_calls, 1);
    assert_eq!(total.cost_micros, 200);
    assert_eq!(store.ai_usage_for_change("untouched-change").unwrap(), None);
}

#[test]
fn human_decisions_keep_their_provenance() {
    let store = open_temp();
    let digest = store.put_blob(b"candidate-requirement").unwrap();
    let decision = HumanDecision::new(NewDecision {
        id: HumanDecisionId::new("hd-1").unwrap(),
        reviewer: "sergii".into(),
        role: HumanRole::Qa,
        subject: "others-visible".into(),
        artifact_digest: digest.clone(),
        decision: VerificationDecision::ObservedOnly,
        comment: Some("behaviour seen, intent unconfirmed".into()),
        decided_at: "2026-08-20T09:00:00Z".into(),
    })
    .unwrap();
    store.put_human_decision(&decision).unwrap();
    let rows = store.human_decisions_for_subject("others-visible").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].reviewer, "sergii");
    assert_eq!(rows[0].role, "qa");
    assert_eq!(rows[0].decision, "observed_only");
    assert_eq!(rows[0].artifact_digest, digest.as_str());
    assert_eq!(rows[0].decided_at, "2026-08-20T09:00:00Z");
    assert!(
        store
            .human_decisions_for_subject("never-reviewed")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn fixed_debt_history_survives_later_runs() {
    let store = open_temp();
    let revision = RevisionId::new("revision-a").unwrap();
    store
        .remember_fixed_debt(&["runtime.unwrap:src/lib.rs:add".into()], &revision)
        .unwrap();
    assert_eq!(
        store.previously_fixed_debt().unwrap(),
        ["runtime.unwrap:src/lib.rs:add"]
    );
}

#[test]
fn observed_only_debt_baseline_survives_a_later_snapshot() {
    let store = open_temp();
    store
        .remember_observed_baseline(
            &["clone.family:src/lib.rs:add".into()],
            &RevisionId::new("revision-b").unwrap(),
            "sankey-others",
        )
        .unwrap();
    store
        .remember_observed_baseline(
            &["clone.family:src/lib.rs:add".into()],
            &RevisionId::new("revision-c").unwrap(),
            "sankey-others",
        )
        .unwrap();
    assert_eq!(
        store.observed_baseline_fingerprints().unwrap(),
        ["clone.family:src/lib.rs:add"]
    );
}
