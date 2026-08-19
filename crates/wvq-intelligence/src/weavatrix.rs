//! Adapter over [`weavatrix_rust`]. The engine owns the graph; we only quote it.

use std::path::Path;

use serde_json::{Map, Value};
use thiserror::Error;
use weavatrix_rust::{Weavatrix, operations};
use wvq_domain::{IdError, RevisionId};

/// Why Weavatrix evidence could not be used.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IntelligenceError {
    /// Engine / scan / parse failure.
    #[error("weavatrix engine: {0}")]
    Engine(String),
    /// Snapshot or operation result had no revision.
    #[error("weavatrix result is missing revision identity")]
    MissingRevision,
    /// Engine and operation reported different revisions for the same call.
    #[error("ambiguous revision identity: snapshot `{expected}`, result `{found}`")]
    AmbiguousRevision {
        /// Revision on the Weavatrix snapshot.
        expected: String,
        /// Revision claimed by the operation payload.
        found: String,
    },
    /// Revision string is empty or otherwise illegal for WVQ.
    #[error("invalid revision: {0}")]
    InvalidRevision(#[from] IdError),
    /// JSON conversion between WVQ (`serde_json`) and the engine failed.
    #[error("evidence JSON: {0}")]
    Json(String),
}

/// Revision-bound handle to a Weavatrix analysis. Not a graph copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoEvidence {
    /// Repository root as reported by Weavatrix.
    pub repository: String,
    /// Content / Git revision from the Weavatrix snapshot.
    pub revision: RevisionId,
    /// Node count quoted from the engine snapshot.
    pub node_count: u64,
    /// Edge count quoted from the engine snapshot.
    pub edge_count: u64,
    /// Engine generator identity (`weavatrix-rust …`).
    pub generator: String,
}

/// Read-only code-evidence port. Implementations must not invent a second graph.
pub trait CodeEvidenceProvider {
    /// Analyze one repository and return revision-bound counts, not a graph.
    ///
    /// # Errors
    ///
    /// Returns [`IntelligenceError`] when the engine fails or revision identity
    /// is missing.
    fn analyze(&self, repo: &Path) -> Result<RepoEvidence, IntelligenceError>;

    /// Run one named Weavatrix operation. The JSON result always carries
    /// `repository` and `revision`.
    ///
    /// # Errors
    ///
    /// Returns [`IntelligenceError`] for unknown operations, engine failures,
    /// or ambiguous revision identity.
    fn operation(
        &self,
        repo: &Path,
        name: &str,
        args: &Value,
    ) -> Result<Value, IntelligenceError>;
}

/// Embeds [`weavatrix_rust`]. No local parser, no local `Graph`.
#[derive(Debug, Default, Clone, Copy)]
pub struct WeavatrixProvider;

impl CodeEvidenceProvider for WeavatrixProvider {
    fn analyze(&self, repo: &Path) -> Result<RepoEvidence, IntelligenceError> {
        let engine = Weavatrix::open(repo).map_err(|err| IntelligenceError::Engine(err.to_string()))?;
        evidence_from_engine(&engine)
    }

    fn operation(
        &self,
        repo: &Path,
        name: &str,
        args: &Value,
    ) -> Result<Value, IntelligenceError> {
        let mut engine =
            Weavatrix::open(repo).map_err(|err| IntelligenceError::Engine(err.to_string()))?;
        let snapshot = engine.state().snapshot();
        let repository = snapshot.repository.clone();
        let revision = require_revision(&snapshot.revision)?;
        let report = operations::call(&mut engine, name, to_engine_value(args)?)
            .map_err(IntelligenceError::Engine)?;
        let mut value = from_engine_value(&report)?;
        attach_identity(&mut value, &repository, revision.as_str())?;
        Ok(value)
    }
}

fn evidence_from_engine(engine: &Weavatrix) -> Result<RepoEvidence, IntelligenceError> {
    let snapshot = engine.state().snapshot();
    Ok(RepoEvidence {
        repository: snapshot.repository.clone(),
        revision: require_revision(&snapshot.revision)?,
        node_count: u64::try_from(snapshot.nodes.len()).unwrap_or(u64::MAX),
        edge_count: u64::try_from(snapshot.edges.len()).unwrap_or(u64::MAX),
        generator: snapshot.generator.clone(),
    })
}

fn require_revision(raw: &str) -> Result<RevisionId, IntelligenceError> {
    if raw.is_empty() {
        return Err(IntelligenceError::MissingRevision);
    }
    Ok(RevisionId::new(raw)?)
}

fn attach_identity(
    value: &mut Value,
    repository: &str,
    revision: &str,
) -> Result<(), IntelligenceError> {
    if !value.is_object() {
        let inner = value.take();
        *value = Value::Object(Map::from_iter([("value".to_owned(), inner)]));
    }
    let object = value
        .as_object_mut()
        .ok_or(IntelligenceError::MissingRevision)?;
    bind_field(object, "repository", repository)?;
    bind_field(object, "revision", revision)?;
    Ok(())
}

fn bind_field(
    object: &mut Map<String, Value>,
    key: &str,
    expected: &str,
) -> Result<(), IntelligenceError> {
    match object.get(key) {
        None => {
            object.insert(key.to_owned(), Value::String(expected.to_owned()));
            Ok(())
        }
        Some(Value::String(found)) if found == expected => Ok(()),
        Some(found) => Err(IntelligenceError::AmbiguousRevision {
            expected: expected.to_owned(),
            found: found.to_string(),
        }),
        // bind_field is only used for revision/repository; a non-string existing
        // field is also ambiguous identity.
    }
}

fn to_engine_value(value: &Value) -> Result<blazingly_json::Value, IntelligenceError> {
    let raw = serde_json::to_string(value).map_err(|err| IntelligenceError::Json(err.to_string()))?;
    blazingly_json::from_str(&raw).map_err(|err| IntelligenceError::Json(err.to_string()))
}

fn from_engine_value(value: &blazingly_json::Value) -> Result<Value, IntelligenceError> {
    let raw =
        blazingly_json::to_string(value).map_err(|err| IntelligenceError::Json(err.to_string()))?;
    serde_json::from_str(&raw).map_err(|err| IntelligenceError::Json(err.to_string()))
}


