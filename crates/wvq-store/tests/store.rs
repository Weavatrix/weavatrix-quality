//! Task 14: CAS dedup, rollback leaves no artifact ref, proofs cannot change.

use std::time::{SystemTime, UNIX_EPOCH};

use wvq_domain::{ArtifactId, ObligationId, OracleSealId, ProofId, RevisionId};
use wvq_store::{Store, StoredProof};

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
    assert_eq!(store.schema_version().unwrap(), 3);
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
