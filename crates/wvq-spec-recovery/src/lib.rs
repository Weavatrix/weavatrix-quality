//! Brownfield spec recovery. Spec §65–§66.
//!
//! Real repositories rarely start with complete `OpenSpec`. This crate turns a
//! pull request, its commits and revision-bound code evidence into *candidate*
//! requirements and acceptance criteria.
//!
//! The governing rule is spec §65.1:
//!
//! > Implementation evidence can propose intent; it cannot establish intent by
//! > itself.
//!
//! Nothing here seals anything. Candidates reach `OracleSeal` only through the
//! mandatory QA verification path.

#![forbid(unsafe_code)]

mod cluster;
mod evidence;
mod narrative;
mod verify;

pub use cluster::{CapabilityCluster, ClusterBasis, CommitFacts, cluster};
pub use evidence::{
    Confidence, ConfidenceLevel, EvidenceSource, EvidenceTier, IntentEvidence, assess,
    establishes_intent, strongest_tier,
};
pub use narrative::{ChangeNarrative, CodeDeltaSummary, NarrativeInput, TestsDelta, narrate};
pub use verify::{
    CandidateRequirement, CandidateShape, FindingKind, ObservedFact, VerifierFinding,
    VerifierReport, VerifyContext, verify_candidates,
};
