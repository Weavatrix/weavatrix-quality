//! `SQLite` connection helpers.

use rusqlite::Connection;
use thiserror::Error;

use crate::migrations::MIGRATIONS;

/// Store / CAS failures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StoreError {
    /// Filesystem error.
    #[error("io error at {path}: {message}")]
    Io {
        /// Path.
        path: String,
        /// OS message.
        message: String,
    },
    /// `SQLite` error.
    #[error("sqlite: {0}")]
    Sqlite(String),
    /// CAS object is missing.
    #[error("missing CAS blob {0}")]
    MissingBlob(String),
    /// Identity / hash could not be formed.
    #[error("invalid store identity: {0}")]
    Invalid(String),
    /// Proofs cannot be mutated.
    #[error("proofs are immutable")]
    ProofImmutable,
}

/// Open (or create) `quality.db` and apply migrations.
///
/// # Errors
///
/// Returns [`StoreError::Sqlite`] on connection or SQL failure.
pub fn open_database(path: &std::path::Path) -> Result<Connection, StoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| StoreError::Io {
            path: parent.display().to_string(),
            message: err.to_string(),
        })?;
    }
    let conn = Connection::open(path).map_err(|err| StoreError::Sqlite(err.to_string()))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|err| StoreError::Sqlite(err.to_string()))?;
    apply_migrations(&conn)?;
    Ok(conn)
}

fn apply_migrations(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )
    .map_err(|err| StoreError::Sqlite(err.to_string()))?;
    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .map_err(|err| StoreError::Sqlite(err.to_string()))?;
    for (index, sql) in MIGRATIONS.iter().enumerate() {
        let version = i64::try_from(index.saturating_add(1)).unwrap_or(i64::MAX);
        if version <= current {
            continue;
        }
        let tx = conn
            .unchecked_transaction()
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        // schema_migrations is created above; 001 also creates it — IF NOT EXISTS is in the helper only.
        // Strip a duplicate schema_migrations create from 001 by running the rest.
        tx.execute_batch(sql)
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, datetime('now'))",
            [version],
        )
        .map_err(|err| StoreError::Sqlite(err.to_string()))?;
        tx.commit()
            .map_err(|err| StoreError::Sqlite(err.to_string()))?;
    }
    Ok(())
}

/// Current schema version.
///
/// # Errors
///
/// Returns [`StoreError::Sqlite`] when the table cannot be read.
pub fn schema_version(conn: &Connection) -> Result<u32, StoreError> {
    let version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .map_err(|err| StoreError::Sqlite(err.to_string()))?;
    u32::try_from(version).map_err(|_| StoreError::Invalid("schema version overflow".into()))
}
