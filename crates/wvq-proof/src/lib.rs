//! Revision-bound Proof assembly. Never a single quality percentage.

#![forbid(unsafe_code)]

mod assemble;
mod differential;
mod verdict;

pub use assemble::{AssemblyInput, ExecutionEvidence, Proof, ProofAssembly, assemble};
pub use differential::{
    CodeDelta, DeltaTriangle, SpecDelta, TriangleAxes, TriangleReading, classify_triangle,
    join_triangle, spec_delta,
};
pub use verdict::{ProofVerdict, VerdictInput, decide_verdict};
