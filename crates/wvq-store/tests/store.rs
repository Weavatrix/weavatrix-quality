//! Task 14: CAS dedup, rollback leaves no artifact ref, proofs cannot change.

use std::time::{SystemTime, UNIX_EPOCH};

use wvq_domain::{
    ArtifactId, HumanDecision, HumanDecisionId, HumanRole, NewDecision, ObligationId, OracleSealId,
    ProofId, RevisionId, VerificationDecision,
};
use wvq_store::{Store, StoredAiUsage, StoredProof, StoredRun, StoredRunItem};

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
    assert_eq!(store.schema_version().unwrap(), 7);
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
    store.put_behavior_state(&digest, body).unwrap();
    store.put_behavior_state(&digest, body).unwrap();
    assert_eq!(store.behavior_state_count().unwrap(), 1);

    let after = store
        .put_blob(br#"{"route":"/analytics/dashboard/42","modal":"others"}"#)
        .unwrap();
    store
        .put_behavior_edge(&digest, &after, "activate")
        .unwrap();
    store
        .put_behavior_edge(&digest, &after, "activate")
        .unwrap();
    assert_eq!(store.behavior_edge_count().unwrap(), 1);

    store
        .put_manual_session("sess-1", Some(7), Some("admin-above-limit"))
        .unwrap();
    let session = store
        .get_manual_session("sess-1")
        .unwrap()
        .expect("session");
    assert_eq!(session.seed, Some(7));
    assert_eq!(session.fixture.as_deref(), Some("admin-above-limit"));
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
