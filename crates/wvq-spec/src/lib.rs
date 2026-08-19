//! `OpenSpec` compatibility reader and (later) quality-contract compilation.
//!
//! WVQ consumes `OpenSpec`. It does not fork the authoring UX.

#![forbid(unsafe_code)]

pub mod openspec;

pub use openspec::{
    Clause, ClauseKind, OpenSpecChange, RequirementDelta, RequirementOp, ScenarioDelta,
    SourceLocation, SpecError, read_change,
};
