//! `SQLite` + content-addressed artifacts. Proofs are immutable; blobs live in CAS.

#![forbid(unsafe_code)]

mod cas;
mod migrations;
mod repository;
mod sqlite;

pub use cas::Cas;
pub use repository::{
    ArtifactRecord, Store, StoredAiUsage, StoredHumanDecision, StoredProof, StoredRun,
    StoredRunItem, StoredSession,
};
pub use sqlite::StoreError;
