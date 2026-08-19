//! High-level evidence ledger: artifacts in CAS, proofs immutable.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use wvq_domain::{ArtifactId, ContentHash, ObligationId, OracleSealId, ProofId, RevisionId};

use crate::cas::Cas;
use crate::sqlite::{self, StoreError};

/// Opened quality store for one repository.
pub struct Store {
    conn: Connection,
    cas: Cas,
    root: PathBuf,
}

/// Artifact metadata. Bytes stay in CAS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRecord {
    /// Artifact id.
    pub id: ArtifactId,
    /// Kind (`junit`, `screenshot`, …).
    pub kind: String,
    /// CAS digest.
    pub content_hash: ContentHash,
    /// Byte length.
    pub byte_len: u64,
}

/// Immutable proof row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredProof {
    /// Proof id.
    pub id: ProofId,
    /// Revision.
    pub revision: RevisionId,
    /// Obligation.
    pub obligation: ObligationId,
    /// Oracle seal.
    pub oracle_seal: OracleSealId,
    /// Verdict token (`PROVEN`, …).
    pub verdict: String,
}

impl Store {
    /// Open `<repo>/.weavatrix-quality/quality.db` and the CAS.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] on IO or SQL failure.
    pub fn open(repo: impl AsRef<Path>) -> Result<Self, StoreError> {
        let quality = repo.as_ref().join(".weavatrix-quality");
        let db = quality.join("quality.db");
        let cas = Cas::open(quality.join("objects"))?;
        let conn = sqlite::open_database(&db)?;
        Ok(Self {
            conn,
            cas,
            root: quality,
        })
    }

    /// Applied schema version.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Sqlite`] if migrations cannot be read.
    pub fn schema_version(&self) -> Result<u32, StoreError> {
        sqlite::schema_version(&self.conn)
    }

    /// Quality directory (db + objects).
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// CAS handle.
    #[must_use]
    pub fn cas(&self) -> &Cas {
        &self.cas
    }

    /// Store bytes in CAS only (deduped).
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Io`] on write failure.
    pub fn put_blob(&self, bytes: &[u8]) -> Result<ContentHash, StoreError> {
        self.cas.put(bytes)
    }

    /// Write blob to CAS then record an artifact row.
    ///
    /// # Errors
    ///
    /// CAS or SQL failure. Never stores the blob in `SQLite`.
    pub fn put_artifact(
        &self,
        id: &ArtifactId,
        kind: &str,
        bytes: &[u8],
    ) -> Result<ContentHash, StoreError> {
        let hash = self.cas.put(bytes)?;
        let len = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
        self.conn
            .execute(
                "INSERT INTO artifacts (id, kind, content_hash, byte_len) VALUES (?1, ?2, ?3, ?4)",
                params![id.as_str(), kind, hash.as_str(), len],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(hash)
    }

    /// Insert an artifact row inside an explicit transaction (for rollback tests).
    ///
    /// # Errors
    ///
    /// SQL failure. The caller owns commit/rollback.
    pub fn insert_artifact_row(
        tx: &rusqlite::Transaction<'_>,
        id: &ArtifactId,
        kind: &str,
        hash: &ContentHash,
        byte_len: u64,
    ) -> Result<(), StoreError> {
        let len = i64::try_from(byte_len).unwrap_or(i64::MAX);
        tx.execute(
            "INSERT INTO artifacts (id, kind, content_hash, byte_len) VALUES (?1, ?2, ?3, ?4)",
            params![id.as_str(), kind, hash.as_str(), len],
        )
        .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Begin a `SQLite` transaction.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Sqlite`].
    pub fn transaction(&mut self) -> Result<rusqlite::Transaction<'_>, StoreError> {
        self.conn
            .transaction()
            .map_err(|err| StoreError::Sqlite(err.to_string()))
    }

    /// Load artifact metadata. Fails closed if the CAS blob is missing.
    ///
    /// # Errors
    ///
    /// [`StoreError::MissingBlob`] when the row exists but CAS does not.
    pub fn get_artifact(&self, id: &ArtifactId) -> Result<Option<ArtifactRecord>, StoreError> {
        let row = self
            .conn
            .query_row(
                "SELECT id, kind, content_hash, byte_len FROM artifacts WHERE id = ?1",
                [id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        let Some((id_raw, kind, hash_raw, len)) = row else {
            return Ok(None);
        };
        let hash =
            ContentHash::new(&hash_raw).map_err(|err| StoreError::Invalid(err.to_string()))?;
        if self.cas.object_path(&hash).is_file() {
            Ok(Some(ArtifactRecord {
                id: ArtifactId::new(id_raw).map_err(|err| StoreError::Invalid(err.to_string()))?,
                kind,
                content_hash: hash,
                byte_len: u64::try_from(len).unwrap_or(0),
            }))
        } else {
            Err(StoreError::MissingBlob(hash_raw))
        }
    }

    /// Insert a proof. Updates are rejected by trigger.
    ///
    /// # Errors
    ///
    /// [`StoreError::ProofImmutable`] on UPDATE, otherwise SQL errors.
    pub fn put_proof(&self, proof: &StoredProof) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO proofs (id, revision, obligation, oracle_seal, verdict) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    proof.id.as_str(),
                    proof.revision.as_str(),
                    proof.obligation.as_str(),
                    proof.oracle_seal.as_str(),
                    proof.verdict.as_str()
                ],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Attempt to mutate a proof (must fail).
    ///
    /// # Errors
    ///
    /// Always [`StoreError::ProofImmutable`] if the trigger fires.
    pub fn update_proof_verdict(&self, id: &ProofId, verdict: &str) -> Result<(), StoreError> {
        match self.conn.execute(
            "UPDATE proofs SET verdict = ?1 WHERE id = ?2",
            params![verdict, id.as_str()],
        ) {
            Ok(_) => Err(StoreError::ProofImmutable),
            Err(err) => {
                let message = err.to_string();
                if message.contains("immutable") {
                    Err(StoreError::ProofImmutable)
                } else {
                    Err(StoreError::Sqlite(message))
                }
            }
        }
    }

    /// Load a proof.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Sqlite`] or [`StoreError::Invalid`].
    pub fn get_proof(&self, id: &ProofId) -> Result<Option<StoredProof>, StoreError> {
        self.conn
            .query_row(
                "SELECT id, revision, obligation, oracle_seal, verdict FROM proofs WHERE id = ?1",
                [id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|err| StoreError::Sqlite(err.to_string()))?
            .map(|(id, revision, obligation, oracle_seal, verdict)| {
                Ok(StoredProof {
                    id: ProofId::new(id).map_err(|err| StoreError::Invalid(err.to_string()))?,
                    revision: RevisionId::new(revision)
                        .map_err(|err| StoreError::Invalid(err.to_string()))?,
                    obligation: ObligationId::new(obligation)
                        .map_err(|err| StoreError::Invalid(err.to_string()))?,
                    oracle_seal: OracleSealId::new(oracle_seal)
                        .map_err(|err| StoreError::Invalid(err.to_string()))?,
                    verdict,
                })
            })
            .transpose()
    }

    /// Persist a hashed behavior state. Body lives in CAS; the row is the digest.
    ///
    /// # Errors
    ///
    /// [`StoreError::Invalid`] when `digest` does not match the content hash.
    pub fn put_behavior_state(&self, digest: &ContentHash, body: &[u8]) -> Result<(), StoreError> {
        let hash = self.cas.put(body)?;
        if hash.as_str() != digest.as_str() {
            return Err(StoreError::Invalid(
                "behavior state digest does not match CAS hash".into(),
            ));
        }
        self.conn
            .execute(
                "INSERT OR IGNORE INTO behavior_states (id, digest) VALUES (?1, ?2)",
                params![digest.as_str(), digest.as_str()],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Persist `src --action--> dst`. Duplicate edges are ignored.
    ///
    /// # Errors
    ///
    /// SQL or hash failure.
    pub fn put_behavior_edge(
        &self,
        src: &ContentHash,
        dst: &ContentHash,
        action: &str,
    ) -> Result<(), StoreError> {
        let id = edge_id(src, dst, action)?;
        self.conn
            .execute(
                "INSERT OR IGNORE INTO behavior_edges (id, src, dst, action) VALUES (?1, ?2, ?3, ?4)",
                params![id, src.as_str(), dst.as_str(), action],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Record a manual QA session so it can be replayed.
    ///
    /// # Errors
    ///
    /// SQL failure.
    pub fn put_manual_session(
        &self,
        id: &str,
        seed: Option<u64>,
        fixture: Option<&str>,
    ) -> Result<(), StoreError> {
        let seed = seed.map(|value| i64::try_from(value).unwrap_or(i64::MAX));
        self.conn
            .execute(
                "INSERT OR REPLACE INTO manual_sessions (id, seed, fixture) VALUES (?1, ?2, ?3)",
                params![id, seed, fixture],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Number of persisted behavior states.
    ///
    /// # Errors
    ///
    /// SQL failure.
    pub fn behavior_state_count(&self) -> Result<u64, StoreError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM behavior_states", [], |row| row.get(0))
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        u64::try_from(count).map_err(|err| StoreError::Invalid(err.to_string()))
    }

    /// Number of persisted behavior edges.
    ///
    /// # Errors
    ///
    /// SQL failure.
    pub fn behavior_edge_count(&self) -> Result<u64, StoreError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM behavior_edges", [], |row| row.get(0))
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        u64::try_from(count).map_err(|err| StoreError::Invalid(err.to_string()))
    }

    /// Load a session's seed and fixture.
    ///
    /// # Errors
    ///
    /// SQL failure.
    pub fn get_manual_session(&self, id: &str) -> Result<Option<StoredSession>, StoreError> {
        self.conn
            .query_row(
                "SELECT seed, fixture FROM manual_sessions WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, Option<i64>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()
            .map_err(|err| StoreError::Sqlite(err.to_string()))?
            .map(|(seed, fixture)| {
                Ok(StoredSession {
                    seed: seed.map(|value| u64::try_from(value).unwrap_or(0)),
                    fixture,
                })
            })
            .transpose()
    }
}

/// Manual session identity stored for replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSession {
    /// Recording seed.
    pub seed: Option<u64>,
    /// Fixture name.
    pub fixture: Option<String>,
}

fn edge_id(src: &ContentHash, dst: &ContentHash, action: &str) -> Result<String, StoreError> {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let mut input = String::new();
    write!(&mut input, "{}|{action}|{}", src.as_str(), dst.as_str())
        .map_err(|err| StoreError::Invalid(err.to_string()))?;
    Ok(Sha256::digest(input.as_bytes())
        .iter()
        .fold(String::new(), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        }))
}
