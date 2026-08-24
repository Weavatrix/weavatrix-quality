//! `OpenSpec` compatibility reader and quality-contract compilation.
//!
//! WVQ consumes `OpenSpec`. It does not fork the authoring UX.

#![forbid(unsafe_code)]

mod delta;
pub mod obligations;
pub mod openspec;
pub mod quality_yaml;
pub mod seal;

pub use delta::{SpecChangeScope, diff_spec_scope};

pub use obligations::{TestObligation, compile_obligations};
pub use openspec::{
    Clause, ClauseKind, OpenSpecChange, RequirementDelta, RequirementOp, ScenarioDelta,
    SourceLocation, SpecError, read_change,
};
pub use quality_yaml::{
    EvidenceKind, ObligationKind, Predicate, PredicateTarget, QualityContract, RiskLevel,
    StorageArea, load_quality_contract,
};
pub use seal::{OracleSeal, seal};
