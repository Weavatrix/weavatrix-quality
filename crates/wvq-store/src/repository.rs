//! High-level evidence ledger: artifacts in CAS, proofs immutable.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use wvq_domain::{
    ArtifactId, ContentHash, HumanDecision, ObligationId, OracleSealId, ProofId, RevisionId, RunId,
};

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

/// AI budget consumption bound to one change and optional run. Spec §26.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StoredAiUsage {
    /// Change the budget belongs to.
    pub change_id: String,
    /// Run, when the usage is run-scoped.
    pub run_id: Option<String>,
    /// Planning tokens consumed.
    pub planning_tokens: u64,
    /// Runtime tokens consumed. Zero on the ordinary green path.
    pub runtime_tokens: u64,
    /// Browser escapes taken.
    pub browser_escape_calls: u64,
    /// Vision calls taken.
    pub vision_calls: u64,
    /// Money consumed, in micros.
    pub cost_micros: u64,
}

/// Recorded human verification row. Spec §66.5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredHumanDecision {
    /// Decision id.
    pub id: String,
    /// Reviewer identity.
    pub reviewer: String,
    /// Reviewer role token.
    pub role: String,
    /// The single subject reviewed.
    pub subject: String,
    /// Digest of what the reviewer saw.
    pub artifact_digest: String,
    /// Decision token.
    pub decision: String,
    /// Optional comment.
    pub comment: Option<String>,
    /// Host-supplied timestamp.
    pub decided_at: String,
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

/// Revision-bound aggregate runner execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRun {
    /// Run identity.
    pub id: RunId,
    /// `OpenSpec` change identity.
    pub change_id: String,
    /// Exact Weavatrix revision before and after execution.
    pub revision: RevisionId,
    /// Lifecycle state (`complete`).
    pub status: String,
    /// True only when every registered executor exited successfully.
    pub passed: bool,
    /// Aggregate outcome (`passed`, `failed`, or `error`).
    pub outcome: String,
}

/// One registered executor inside an aggregate run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRunItem {
    /// Stable item identity.
    pub id: String,
    /// Parent run.
    pub run_id: RunId,
    /// Frozen executor registry id.
    pub executor: String,
    /// Process exit code, when the process exited normally.
    pub status_code: Option<i32>,
    /// Successful executor outcome.
    pub passed: bool,
}

/// One normalized test-case result bound to an exact run and revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTestCaseResult {
    /// Stable occurrence identity.
    pub id: String,
    /// Parent aggregate run.
    pub run_id: RunId,
    /// Exact repository revision observed by the runner.
    pub revision: RevisionId,
    /// Frozen executor registry id.
    pub executor: String,
    /// Runner-reported suite, package, or file.
    pub suite: String,
    /// Runner-reported case name.
    pub name: String,
    /// `pass`, `fail`, `skip`, or `error`.
    pub status: String,
    /// Runner-reported duration, when available.
    pub duration_ms: Option<u64>,
    /// Stable failure fingerprint, when this occurrence failed.
    pub fingerprint: Option<ContentHash>,
}

/// Historical analytics for one executor/suite/test identity.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TestCaseStats {
    /// Total recorded occurrences.
    pub runs: u64,
    /// Successful occurrences.
    pub passes: u64,
    /// Assertion failures.
    pub failures: u64,
    /// Runner/infrastructure errors.
    pub errors: u64,
    /// Skipped occurrences.
    pub skips: u64,
    /// Mean of runner-reported durations, excluding missing durations.
    pub average_duration_ms: Option<u64>,
    /// True only after this exact identity has both passed and failed/errored.
    pub flaky: bool,
}

/// A test repeatedly observed covering one or more requested graph nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalTestCandidate {
    /// Repository-relative test path.
    pub test_path: String,
    /// Requested nodes with repeated measured coverage.
    pub matched_nodes: Vec<String>,
    /// Weakest observation count among the matched nodes.
    pub minimum_observations: u64,
    /// Deterministically selected exact revision from the matching observations.
    pub last_revision: RevisionId,
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

    /// Read artifact bytes from CAS after validating its ledger row.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::MissingBlob`] when either the row or CAS object is absent.
    pub fn read_artifact(&self, id: &ArtifactId) -> Result<(ArtifactRecord, Vec<u8>), StoreError> {
        let record = self
            .get_artifact(id)?
            .ok_or_else(|| StoreError::MissingBlob(id.to_string()))?;
        let bytes = self.cas.get(&record.content_hash)?;
        Ok((record, bytes))
    }

    /// Persist an aggregate execution after its artifacts are safely in CAS.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Sqlite`] on duplicate identity or database failure.
    pub fn put_run(&self, run: &StoredRun) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO runs (id, executor, change_id, revision, status, passed, outcome) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    run.id.as_str(),
                    "aggregate",
                    run.change_id,
                    run.revision.as_str(),
                    run.status,
                    i64::from(run.passed),
                    run.outcome,
                ],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Persist one executor outcome for a run.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Sqlite`] on invalid references or database failure.
    pub fn put_run_item(&self, item: &StoredRunItem) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO run_items (id, run_id, executor, status_code, passed) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    item.id,
                    item.run_id.as_str(),
                    item.executor,
                    item.status_code,
                    i64::from(item.passed),
                ],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Persist one normalized case result for historical test analytics.
    ///
    /// # Errors
    ///
    /// Rejects unknown status tokens and returns SQL/identity failures.
    pub fn put_test_case_result(&self, item: &StoredTestCaseResult) -> Result<(), StoreError> {
        if !matches!(item.status.as_str(), "pass" | "fail" | "skip" | "error") {
            return Err(StoreError::Invalid(format!(
                "unknown test case status `{}`",
                item.status
            )));
        }
        let duration_ms = item.duration_ms.map(to_i64).transpose()?;
        self.conn
            .execute(
                "INSERT INTO test_case_results \
                 (id, run_id, revision, executor, suite, name, status, duration_ms, fingerprint) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    item.id,
                    item.run_id.as_str(),
                    item.revision.as_str(),
                    item.executor,
                    item.suite,
                    item.name,
                    item.status,
                    duration_ms,
                    item.fingerprint.as_ref().map(ContentHash::as_str),
                ],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Aggregate duration and pass/fail history for one exact test identity.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Sqlite`] if the analytics table cannot be read.
    pub fn test_case_stats(
        &self,
        executor: &str,
        suite: &str,
        name: &str,
    ) -> Result<TestCaseStats, StoreError> {
        let row = self
            .conn
            .query_row(
                "SELECT COUNT(*), \
                 COALESCE(SUM(CASE WHEN status = 'pass' THEN 1 ELSE 0 END), 0), \
                 COALESCE(SUM(CASE WHEN status = 'fail' THEN 1 ELSE 0 END), 0), \
                 COALESCE(SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END), 0), \
                 COALESCE(SUM(CASE WHEN status = 'skip' THEN 1 ELSE 0 END), 0), \
                 COALESCE(SUM(duration_ms), 0), COUNT(duration_ms) \
                 FROM test_case_results WHERE executor = ?1 AND suite = ?2 AND name = ?3",
                params![executor, suite, name],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        let passes = to_u64(row.1);
        let failures = to_u64(row.2);
        let errors = to_u64(row.3);
        let duration_count = to_u64(row.6);
        Ok(TestCaseStats {
            runs: to_u64(row.0),
            passes,
            failures,
            errors,
            skips: to_u64(row.4),
            average_duration_ms: (duration_count > 0).then(|| to_u64(row.5) / duration_count),
            flaky: passes > 0 && failures.saturating_add(errors) > 0,
        })
    }

    /// Add one exact-test coverage observation for each measured graph node.
    ///
    /// Callers must only attribute aggregate coverage when exactly one test path
    /// was executed; the store deliberately does not infer that condition.
    ///
    /// # Errors
    ///
    /// Returns SQL or identity failures.
    pub fn observe_test_nodes(
        &self,
        run_id: &RunId,
        test_path: &str,
        node_ids: &[String],
        revision: &RevisionId,
    ) -> Result<(), StoreError> {
        if test_path.trim().is_empty() {
            return Err(StoreError::Invalid("test path cannot be empty".into()));
        }
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        for node in node_ids
            .iter()
            .filter(|node| !node.trim().is_empty())
            .collect::<BTreeSet<_>>()
        {
            let inserted = tx
                .execute(
                    "INSERT OR IGNORE INTO test_node_observation_runs \
                     (test_path, node_id, run_id, revision) VALUES (?1, ?2, ?3, ?4)",
                    params![test_path, node, run_id.as_str(), revision.as_str()],
                )
                .map_err(|err| StoreError::Sqlite(err.to_string()))?;
            if inserted == 0 {
                continue;
            }
            tx.execute(
                "INSERT INTO test_node_observations \
                     (test_path, node_id, observations, last_revision) VALUES (?1, ?2, 1, ?3) \
                     ON CONFLICT(test_path, node_id) DO UPDATE SET \
                     observations = observations + 1, last_revision = excluded.last_revision",
                params![test_path, node, revision.as_str()],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        }
        tx.commit()
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Find tests with repeated exact-test coverage of requested graph nodes.
    ///
    /// # Errors
    ///
    /// Rejects unbounded/invalid queries and returns SQL or revision failures.
    pub fn historical_tests_for_nodes(
        &self,
        node_ids: &[String],
        minimum_observations: u64,
        max_rows: usize,
    ) -> Result<Vec<HistoricalTestCandidate>, StoreError> {
        if minimum_observations == 0 || max_rows == 0 {
            return Err(StoreError::Invalid(
                "selection history requires positive observation and row limits".into(),
            ));
        }
        let requested = node_ids
            .iter()
            .filter(|node| !node.trim().is_empty())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut merged = BTreeMap::<String, (BTreeSet<String>, u64, String)>::new();
        let threshold = to_i64(minimum_observations)?;
        let mut matched_rows = 0_usize;
        for chunk in requested.iter().collect::<Vec<_>>().chunks(250) {
            let placeholders = std::iter::repeat_n("?", chunk.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT test_path, node_id, observations, last_revision \
                 FROM test_node_observations WHERE observations >= ? \
                 AND node_id IN ({placeholders}) ORDER BY test_path, node_id"
            );
            let mut values = Vec::<rusqlite::types::Value>::with_capacity(chunk.len() + 1);
            values.push(rusqlite::types::Value::Integer(threshold));
            values.extend(
                chunk
                    .iter()
                    .map(|node| rusqlite::types::Value::Text((*node).clone())),
            );
            let mut statement = self
                .conn
                .prepare(&sql)
                .map_err(|err| StoreError::Sqlite(err.to_string()))?;
            let rows = statement
                .query_map(rusqlite::params_from_iter(values), |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(|err| StoreError::Sqlite(err.to_string()))?;
            for row in rows {
                let (path, node, observations, revision) =
                    row.map_err(|err| StoreError::Sqlite(err.to_string()))?;
                matched_rows = matched_rows.saturating_add(1);
                if matched_rows > max_rows {
                    return Err(StoreError::Invalid(format!(
                        "selection history exceeds {max_rows} matching rows"
                    )));
                }
                let observations = to_u64(observations);
                let entry = merged
                    .entry(path)
                    .or_insert_with(|| (BTreeSet::new(), observations, revision.clone()));
                entry.0.insert(node);
                entry.1 = entry.1.min(observations);
                if revision > entry.2 {
                    entry.2 = revision;
                }
            }
        }
        merged
            .into_iter()
            .map(
                |(test_path, (matched_nodes, minimum_observations, revision))| {
                    Ok(HistoricalTestCandidate {
                        test_path,
                        matched_nodes: matched_nodes.into_iter().collect(),
                        minimum_observations,
                        last_revision: RevisionId::new(revision)
                            .map_err(|err| StoreError::Invalid(err.to_string()))?,
                    })
                },
            )
            .collect()
    }

    /// Link an artifact to its producing run.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Sqlite`] on invalid references or database failure.
    pub fn attach_run_artifact(
        &self,
        run: &RunId,
        artifact: &ArtifactId,
    ) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO run_artifacts (run_id, artifact) VALUES (?1, ?2)",
                params![run.as_str(), artifact.as_str()],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Load one run by id.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] for SQL or identity failures.
    pub fn get_run(&self, id: &RunId) -> Result<Option<StoredRun>, StoreError> {
        self.query_run(
            "SELECT id, change_id, revision, status, passed, outcome FROM runs WHERE id = ?1",
            id.as_str(),
        )
    }

    /// Latest recorded run for a change and exact revision.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] for SQL or identity failures.
    pub fn latest_run(
        &self,
        change_id: &str,
        revision: &RevisionId,
    ) -> Result<Option<StoredRun>, StoreError> {
        let row = self
            .conn
            .query_row(
                "SELECT id, change_id, revision, status, passed, outcome FROM runs WHERE change_id = ?1 AND revision = ?2 ORDER BY rowid DESC LIMIT 1",
                params![change_id, revision.as_str()],
                decode_run,
            )
            .optional()
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        row.map(parse_run).transpose()
    }

    /// Latest run in this repository, regardless of change or revision.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] for SQL or identity failures.
    pub fn latest_run_any(&self) -> Result<Option<StoredRun>, StoreError> {
        let row = self
            .conn
            .query_row(
                "SELECT id, change_id, revision, status, passed, outcome FROM runs ORDER BY rowid DESC LIMIT 1",
                [],
                decode_run,
            )
            .optional()
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        row.map(parse_run).transpose()
    }

    /// Artifact ids attached to a run, in stable order.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] for SQL or identity failures.
    pub fn run_artifacts(&self, run: &RunId) -> Result<Vec<ArtifactId>, StoreError> {
        let mut statement = self
            .conn
            .prepare("SELECT artifact FROM run_artifacts WHERE run_id = ?1 ORDER BY artifact")
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        let rows = statement
            .query_map([run.as_str()], |row| row.get::<_, String>(0))
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        let mut out = Vec::new();
        for row in rows {
            let raw = row.map_err(|err| StoreError::Sqlite(err.to_string()))?;
            out.push(ArtifactId::new(raw).map_err(|err| StoreError::Invalid(err.to_string()))?);
        }
        Ok(out)
    }

    /// Remember debt fingerprints that disappeared at a revision.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the ledger write fails.
    pub fn remember_fixed_debt(
        &self,
        fingerprints: &[String],
        revision: &RevisionId,
    ) -> Result<(), StoreError> {
        let mut statement = self
            .conn
            .prepare(
                "INSERT INTO debt_history (fingerprint, state, revision)
                 VALUES (?1, 'fixed', ?2)
                 ON CONFLICT(fingerprint) DO UPDATE
                 SET state = 'fixed', revision = excluded.revision",
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        for fingerprint in fingerprints {
            statement
                .execute(params![fingerprint, revision.as_str()])
                .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        }
        Ok(())
    }

    /// Every debt fingerprint recorded in a fixed bucket.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the ledger query fails.
    pub fn previously_fixed_debt(&self) -> Result<Vec<String>, StoreError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT fingerprint FROM debt_history WHERE state = 'fixed' ORDER BY fingerprint",
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| StoreError::Sqlite(err.to_string()))
    }

    fn query_run(&self, sql: &str, value: &str) -> Result<Option<StoredRun>, StoreError> {
        let row = self
            .conn
            .query_row(sql, [value], decode_run)
            .optional()
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        row.map(parse_run).transpose()
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

    /// Load the latest proof for one obligation at an exact revision.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Sqlite`] or [`StoreError::Invalid`].
    pub fn proof_for_obligation(
        &self,
        revision: &RevisionId,
        obligation: &ObligationId,
    ) -> Result<Option<StoredProof>, StoreError> {
        self.conn
            .query_row(
                "SELECT id, revision, obligation, oracle_seal, verdict FROM proofs \
                 WHERE revision = ?1 AND obligation = ?2 ORDER BY rowid DESC LIMIT 1",
                params![revision.as_str(), obligation.as_str()],
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

    /// Persist a failure fingerprint. Repeats share the same digest.
    ///
    /// # Errors
    ///
    /// SQL failure.
    pub fn put_failure_fingerprint(
        &self,
        digest: &ContentHash,
        class: &str,
    ) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO failure_fingerprints (id, digest, class) VALUES (?1, ?2, ?3)",
                params![digest.as_str(), digest.as_str(), class],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Record one occurrence of a fingerprint.
    ///
    /// # Errors
    ///
    /// SQL failure.
    pub fn put_failure_occurrence(&self, id: &str, digest: &ContentHash) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO failure_occurrences (id, fingerprint, seen_at) VALUES (?1, ?2, datetime('now'))",
                params![id, digest.as_str()],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// How many occurrences share this fingerprint.
    ///
    /// # Errors
    ///
    /// SQL failure.
    pub fn failure_cluster_size(&self, digest: &ContentHash) -> Result<u64, StoreError> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM failure_occurrences WHERE fingerprint = ?1",
                [digest.as_str()],
                |row| row.get(0),
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        u64::try_from(count).map_err(|err| StoreError::Invalid(err.to_string()))
    }

    /// Store a healed program revision bound to its `OracleSeal`.
    ///
    /// # Errors
    ///
    /// SQL failure.
    pub fn put_program_revision(
        &self,
        program: &str,
        revision: u32,
        seal: &str,
    ) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO program_revisions (program, revision, seal) VALUES (?1, ?2, ?3)",
                params![program, i64::from(revision), seal],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Latest stored revision for a program.
    ///
    /// # Errors
    ///
    /// SQL failure.
    pub fn latest_program_revision(&self, program: &str) -> Result<Option<u32>, StoreError> {
        let row: Option<i64> = self
            .conn
            .query_row(
                "SELECT MAX(revision) FROM program_revisions WHERE program = ?1",
                [program],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(row.and_then(|value| u32::try_from(value).ok()))
    }

    /// Persist a mutant and its killed/survived status.
    ///
    /// # Errors
    ///
    /// SQL failure.
    pub fn put_mutation_result(
        &self,
        case_id: &str,
        operator: &str,
        region: &str,
        status: &str,
    ) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mutation_cases (id, operator, region) VALUES (?1, ?2, ?3)",
                params![case_id, operator, region],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mutation_results (id, case_id, status) VALUES (?1, ?2, ?3)",
                params![format!("res-{case_id}"), case_id, status],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Recorded killed/survived status for one mutant.
    ///
    /// # Errors
    ///
    /// SQL failure.
    pub fn mutation_status(&self, case_id: &str) -> Result<Option<String>, StoreError> {
        self.conn
            .query_row(
                "SELECT status FROM mutation_results WHERE case_id = ?1",
                [case_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| StoreError::Sqlite(err.to_string()))
    }

    /// Persist AI budget consumption for one change or run.
    ///
    /// # Errors
    ///
    /// SQL failure.
    pub fn put_ai_usage(&self, id: &str, usage: &StoredAiUsage) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO ai_usage \
                 (id, tokens, change_id, run_id, planning_tokens, runtime_tokens, \
                  browser_escape_calls, vision_calls, cost_micros) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id,
                    to_i64(usage.planning_tokens.saturating_add(usage.runtime_tokens))?,
                    usage.change_id,
                    usage.run_id.clone().unwrap_or_default(),
                    to_i64(usage.planning_tokens)?,
                    to_i64(usage.runtime_tokens)?,
                    to_i64(usage.browser_escape_calls)?,
                    to_i64(usage.vision_calls)?,
                    to_i64(usage.cost_micros)?,
                ],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Total AI usage recorded for one change. Absent evidence is `None`.
    ///
    /// # Errors
    ///
    /// SQL failure.
    pub fn ai_usage_for_change(
        &self,
        change_id: &str,
    ) -> Result<Option<StoredAiUsage>, StoreError> {
        let row = self
            .conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(planning_tokens), 0), \
                 COALESCE(SUM(runtime_tokens), 0), COALESCE(SUM(browser_escape_calls), 0), \
                 COALESCE(SUM(vision_calls), 0), COALESCE(SUM(cost_micros), 0) \
                 FROM ai_usage WHERE change_id = ?1",
                [change_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        if row.0 == 0 {
            return Ok(None);
        }
        Ok(Some(StoredAiUsage {
            change_id: change_id.to_owned(),
            run_id: None,
            planning_tokens: to_u64(row.1),
            runtime_tokens: to_u64(row.2),
            browser_escape_calls: to_u64(row.3),
            vision_calls: to_u64(row.4),
            cost_micros: to_u64(row.5),
        }))
    }

    /// Persist one provenance-bearing human decision.
    ///
    /// The domain type already refused bulk subjects, so a single row can never
    /// stand for an implicit "accept all".
    ///
    /// # Errors
    ///
    /// SQL failure.
    pub fn put_human_decision(&self, decision: &HumanDecision) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO human_decisions \
                 (id, reviewer, role, subject, artifact_digest, decision, comment, decided_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    decision.id.as_str(),
                    decision.reviewer,
                    decision.role.as_str(),
                    decision.subject,
                    decision.artifact_digest.as_str(),
                    decision.decision.as_str(),
                    decision.comment,
                    decision.decided_at,
                ],
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Every decision recorded for one subject, oldest row first.
    ///
    /// # Errors
    ///
    /// SQL failure.
    pub fn human_decisions_for_subject(
        &self,
        subject: &str,
    ) -> Result<Vec<StoredHumanDecision>, StoreError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT id, reviewer, role, subject, artifact_digest, decision, comment, decided_at \
                 FROM human_decisions WHERE subject = ?1 ORDER BY id",
            )
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        let rows = statement
            .query_map([subject], |row| {
                Ok(StoredHumanDecision {
                    id: row.get(0)?,
                    reviewer: row.get(1)?,
                    role: row.get(2)?,
                    subject: row.get(3)?,
                    artifact_digest: row.get(4)?,
                    decision: row.get(5)?,
                    comment: row.get(6)?,
                    decided_at: row.get(7)?,
                })
            })
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|err| StoreError::Sqlite(err.to_string()))?);
        }
        Ok(out)
    }
}

type RunRow = (String, String, String, String, i64, String);

fn decode_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn parse_run(row: RunRow) -> Result<StoredRun, StoreError> {
    Ok(StoredRun {
        id: RunId::new(row.0).map_err(|err| StoreError::Invalid(err.to_string()))?,
        change_id: row.1,
        revision: RevisionId::new(row.2).map_err(|err| StoreError::Invalid(err.to_string()))?,
        status: row.3,
        passed: row.4 != 0,
        outcome: row.5,
    })
}

fn to_i64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|err| StoreError::Invalid(err.to_string()))
}

fn to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
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
