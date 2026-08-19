//! Stable typed contracts for Weavatrix Quality.
//!
//! This crate owns identity, finding, and ratchet enums used by every other
//! WVQ crate. It does not parse `OpenSpec`, talk to Weavatrix, or execute tests.

#![forbid(unsafe_code)]

mod decision;
mod finding;
mod ids;

pub use decision::{DecisionError, HumanDecision, HumanRole, NewDecision, VerificationDecision};
pub use finding::{DebtFingerprint, DebtState, FindingState, QualityFinding, Severity, SubjectRef};
pub use ids::{
    ArtifactId, ChangeId, CheckId, ContentHash, HumanDecisionId, IdError, ObligationId,
    OracleSealId, ProgramId, ProofId, RequirementId, RevisionId, RunId, ScenarioId,
};
