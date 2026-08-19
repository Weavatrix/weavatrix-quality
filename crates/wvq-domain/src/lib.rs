//! Stable typed contracts for Weavatrix Quality.
//!
//! This crate owns identity, finding, and ratchet enums used by every other
//! WVQ crate. It does not parse `OpenSpec`, talk to Weavatrix, or execute tests.

#![forbid(unsafe_code)]

mod finding;
mod ids;

pub use finding::{DebtState, FindingState, QualityFinding, Severity, SubjectRef};
pub use ids::{
    ArtifactId, ChangeId, CheckId, ContentHash, IdError, ObligationId, OracleSealId, ProgramId,
    ProofId, RequirementId, RunId, ScenarioId,
};
