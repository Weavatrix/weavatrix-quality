//! Command-bus failure. Unknown values fail closed.

use thiserror::Error;
use wvq_spec::SpecError;

/// Command-bus failure. Unknown values fail closed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BusError {
    /// `OpenSpec` / `quality.yaml` error.
    #[error(transparent)]
    Spec(#[from] SpecError),
    /// Requested entity does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// `current` matched more than one change.
    #[error("{0}")]
    Ambiguous(String),
    /// Unknown enum / policy token.
    #[error("unknown {field} `{value}`")]
    Unknown {
        /// Field name.
        field: &'static str,
        /// Rejected token.
        value: String,
    },
    /// Identity or revision could not be formed.
    #[error("invalid identity: {0}")]
    Identity(String),
    /// Caller-supplied command or candidate failed strict validation.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// Registered runner discovery, preparation, or execution failed.
    #[error("runtime: {0}")]
    Runtime(String),
    /// Revision-bound Weavatrix evidence failed.
    #[error("intelligence: {0}")]
    Intelligence(String),
    /// Evidence ledger or CAS failed.
    #[error("store: {0}")]
    Store(String),
    /// Explicit loopback model call or AI Cost Firewall failed.
    #[error("model: {0}")]
    Model(String),
}
