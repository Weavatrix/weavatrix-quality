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
    assert_eq!(store.schema_version().unwrap(), 1);
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
