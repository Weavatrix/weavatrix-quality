//! Revision-bound Proof assembly. Never a single quality percentage.

#![forbid(unsafe_code)]

mod assemble;
mod verdict;

pub use assemble::{AssemblyInput, ExecutionEvidence, Proof, ProofAssembly, assemble};
pub use verdict::{ProofVerdict, VerdictInput, decide_verdict};
