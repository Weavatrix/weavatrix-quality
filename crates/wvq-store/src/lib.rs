//! `SQLite` + content-addressed artifacts. Proofs are immutable; blobs live in CAS.

#![forbid(unsafe_code)]

mod cas;
mod migrations;
mod repository;
mod sqlite;

pub use cas::Cas;
pub use repository::{
    ArtifactRecord, HistoricalTestCandidate, Store, StoredAiUsage, StoredHumanDecision,
    StoredProof, StoredRun, StoredRunItem, StoredSession, StoredTestCaseResult, TestCaseStats,
};
pub use sqlite::StoreError;
